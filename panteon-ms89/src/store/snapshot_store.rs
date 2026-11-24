use super::{parse_key_value, strip_quotes};
use crate::model::StateSnapshot;
use crate::{current_timestamp_secs, PanteonError, FEATURE_REPORT_LEN};

/// Serialize StateSnapshot into minimal TOML string format
pub fn snapshot_to_toml_string(snapshot: &StateSnapshot) -> String {
    let mut out = String::new();
    out.push_str(&format!("source_pid = \"0x{:04x}\"\n", snapshot.source_pid));
    out.push_str(&format!("captured_at = \"{}\"\n", snapshot.captured_at));
    out.push_str(&format!("label = \"{}\"\n\n", snapshot.label));

    for (frame, delay) in snapshot.frames.iter().zip(snapshot.delays_ms.iter()) {
        out.push_str("[[frames]]\n");
        let hex_data = frame.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
        out.push_str(&format!("data = \"{}\"\n", hex_data));
        out.push_str(&format!("delay_ms = {}\n\n", delay));
    }

    out
}

enum SnapshotSection {
    Root,
    Frames,
}

#[derive(Default)]
struct FrameBuilder {
    data: Option<[u8; FEATURE_REPORT_LEN]>,
    delay_ms: Option<u16>,
}

impl FrameBuilder {
    fn build(self) -> Result<([u8; FEATURE_REPORT_LEN], u16), PanteonError> {
        let data = self.data.ok_or_else(|| PanteonError::ProtocolDetailed {
            context: "StateSnapshot",
            detail: "missing required field 'data' in frame".to_string(),
        })?;
        let delay_ms = self.delay_ms.ok_or_else(|| PanteonError::ProtocolDetailed {
            context: "StateSnapshot",
            detail: "missing required field 'delay_ms' in frame".to_string(),
        })?;
        Ok((data, delay_ms))
    }
}

/// Parse minimal TOML string into StateSnapshot
pub fn parse_snapshot(toml_str: &str) -> Result<StateSnapshot, PanteonError> {
    let mut source_pid: Option<u16> = None;
    let mut captured_at: Option<String> = None;
    let mut label: Option<String> = None;

    let mut frames = Vec::new();
    let mut delays_ms = Vec::new();
    let mut current_frame: Option<FrameBuilder> = None;
    let mut current_section = SnapshotSection::Root;

    for line in toml_str.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line == "[[frames]]" {
            if let Some(builder) = current_frame.take() {
                let (f, d) = builder.build()?;
                frames.push(f);
                delays_ms.push(d);
            }
            current_section = SnapshotSection::Frames;
            current_frame = Some(FrameBuilder::default());
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            return Err(PanteonError::ProtocolDetailed {
                context: "StateSnapshot",
                detail: format!("unknown section '{}'", line),
            });
        }

        let (key, val) = parse_key_value(line)?;
        match current_section {
            SnapshotSection::Root => match key {
                "source_pid" => {
                    let s = strip_quotes(val);
                    let pid = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                        u16::from_str_radix(hex, 16)
                    } else {
                        s.parse::<u16>()
                    }
                    .map_err(|_| PanteonError::ProtocolDetailed {
                        context: "StateSnapshot",
                        detail: format!("invalid source_pid value '{}'", val),
                    })?;
                    source_pid = Some(pid);
                }
                "captured_at" => {
                    captured_at = Some(strip_quotes(val).to_string());
                }
                "label" => {
                    label = Some(strip_quotes(val).to_string());
                }
                _ => {}
            },
            SnapshotSection::Frames => {
                if let Some(ref mut b) = current_frame {
                    match key {
                        "data" => {
                            let s = strip_quotes(val);
                            let bytes_vec = parse_hex_bytes(s)?;
                            if bytes_vec.len() != FEATURE_REPORT_LEN {
                                return Err(PanteonError::ProtocolDetailed {
                                    context: "StateSnapshot",
                                    detail: format!(
                                        "frame data must be exactly {} bytes, got {}",
                                        FEATURE_REPORT_LEN,
                                        bytes_vec.len()
                                    ),
                                });
                            }
                            let mut arr = [0u8; FEATURE_REPORT_LEN];
                            arr.copy_from_slice(&bytes_vec);
                            b.data = Some(arr);
                        }
                        "delay_ms" => {
                            let d = val.parse::<u16>().map_err(|_| PanteonError::ProtocolDetailed {
                                context: "StateSnapshot",
                                detail: format!("invalid delay_ms value '{}'", val),
                            })?;
                            b.delay_ms = Some(d);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if let Some(builder) = current_frame.take() {
        let (f, d) = builder.build()?;
        frames.push(f);
        delays_ms.push(d);
    }

    let source_pid = source_pid.ok_or_else(|| PanteonError::ProtocolDetailed {
        context: "StateSnapshot",
        detail: "missing required field 'source_pid'".to_string(),
    })?;
    let captured_at = captured_at.unwrap_or_else(current_timestamp_secs);
    let label = label.unwrap_or_default();

    StateSnapshot::new(source_pid, captured_at, label, frames, delays_ms)
}

fn parse_hex_bytes(s: &str) -> Result<Vec<u8>, PanteonError> {
    let clean = s.replace(' ', "");
    if clean.len() % 2 != 0 {
        return Err(PanteonError::ProtocolDetailed {
            context: "parse_hex_bytes",
            detail: format!("odd number of hex digits in '{}'", s),
        });
    }
    let mut bytes = Vec::with_capacity(clean.len() / 2);
    for i in (0..clean.len()).step_by(2) {
        let byte = u8::from_str_radix(&clean[i..i + 2], 16).map_err(|_| PanteonError::ProtocolDetailed {
            context: "parse_hex_bytes",
            detail: format!("invalid hex byte in '{}'", s),
        })?;
        bytes.push(byte);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::to_toml_string;

    #[test]
    fn snapshot_toml_roundtrip() {
        let snapshot = StateSnapshot::new(
            0x1440,
            "1712345678",
            "test_snapshot",
            vec![
                [0x07, 0x18, 0x03, 0x00, 0x01, 0x00, 0x03, 0x00],
                [0x07, 0x18, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00],
            ],
            vec![0, 32],
        )
        .unwrap();

        let toml_str = to_toml_string(&snapshot);
        let parsed = parse_snapshot(&toml_str).expect("Failed to parse serialized snapshot");
        assert_eq!(snapshot, parsed);
    }

    #[test]
    fn parse_snapshot_missing_pid_fails() {
        let toml_no_pid = r#"
captured_at = "1712345678"
label = "no_pid"

[[frames]]
data = "07 18 03 00 01 00 03 00"
delay_ms = 0
"#;
        let err = parse_snapshot(toml_no_pid).unwrap_err();
        assert_eq!(
            err,
            PanteonError::ProtocolDetailed {
                context: "StateSnapshot",
                detail: "missing required field 'source_pid'".to_string(),
            }
        );
    }

    #[test]
    fn parse_snapshot_invalid_frame_data_fails() {
        let toml_bad_data = r#"
source_pid = "0x1440"
captured_at = "1712345678"
label = "bad_frame"

[[frames]]
data = "07 18 03"
delay_ms = 0
"#;
        let err = parse_snapshot(toml_bad_data).unwrap_err();
        match err {
            PanteonError::ProtocolDetailed { context, detail } => {
                assert_eq!(context, "StateSnapshot");
                assert!(detail.contains("frame data must be exactly 8 bytes"));
            }
            err => panic!("Unexpected error: {:?}", err),
        }
    }
}
