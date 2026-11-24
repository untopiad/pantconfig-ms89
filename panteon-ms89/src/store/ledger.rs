use super::{parse_key_value, strip_quotes};
use crate::model::{ShadowEntry, ShadowLedger};
use crate::PanteonError;

/// Serialize ShadowLedger into minimal TOML string format
pub fn ledger_to_toml_string(ledger: &ShadowLedger) -> String {
    let mut out = String::new();
    out.push_str(&format!("device_hint = \"{}\"\n\n", ledger.device_hint));
    for entry in &ledger.entries {
        out.push_str("[[entries]]\n");
        out.push_str(&format!("bank = {}\n", entry.bank));
        out.push_str(&format!("addr = {}\n", entry.addr));
        out.push_str(&format!("data = {}\n", entry.data));
        out.push_str(&format!("written_at = {}\n\n", entry.written_at));
    }
    out
}

enum LedgerSection {
    Root,
    Entries,
}

#[derive(Default)]
struct ShadowEntryBuilder {
    bank: Option<u8>,
    addr: Option<u16>,
    data: Option<u16>,
    written_at: Option<u64>,
}

impl ShadowEntryBuilder {
    fn build(self) -> Result<ShadowEntry, PanteonError> {
        let bank = self.bank.ok_or(PanteonError::ProtocolDetailed {
            context: "ShadowEntry",
            detail: "missing required field 'bank'".to_string(),
        })?;
        let addr = self.addr.ok_or(PanteonError::ProtocolDetailed {
            context: "ShadowEntry",
            detail: "missing required field 'addr'".to_string(),
        })?;
        let data = self.data.ok_or(PanteonError::ProtocolDetailed {
            context: "ShadowEntry",
            detail: "missing required field 'data'".to_string(),
        })?;
        let written_at = self.written_at.ok_or(PanteonError::ProtocolDetailed {
            context: "ShadowEntry",
            detail: "missing required field 'written_at'".to_string(),
        })?;
        Ok(ShadowEntry {
            bank,
            addr,
            data,
            written_at,
        })
    }
}

/// Parse minimal TOML string into ShadowLedger
pub fn parse_ledger(toml_str: &str) -> Result<ShadowLedger, PanteonError> {
    let mut device_hint = String::from("unknown");
    let mut entries = Vec::new();
    let mut current_entry: Option<ShadowEntryBuilder> = None;
    let mut current_section = LedgerSection::Root;

    for line in toml_str.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line == "[[entries]]" {
            if let Some(builder) = current_entry.take() {
                entries.push(builder.build()?);
            }
            current_section = LedgerSection::Entries;
            current_entry = Some(ShadowEntryBuilder::default());
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            return Err(PanteonError::ProtocolDetailed {
                context: "ShadowLedger",
                detail: format!("unknown section '{}'", line),
            });
        }

        let (key, val) = parse_key_value(line)?;
        match current_section {
            LedgerSection::Root => match key {
                "device_hint" => device_hint = strip_quotes(val).to_string(),
                _ => {} // unknown key ignored for compatibility
            },
            LedgerSection::Entries => {
                if let Some(ref mut b) = current_entry {
                    match key {
                        "bank" => {
                            b.bank = Some(val.parse::<u8>().map_err(|_| PanteonError::ProtocolDetailed {
                                context: "ShadowEntry",
                                detail: format!("invalid bank value '{}'", val),
                            })?)
                        }
                        "addr" => {
                            b.addr = Some(val.parse::<u16>().map_err(|_| PanteonError::ProtocolDetailed {
                                context: "ShadowEntry",
                                detail: format!("invalid addr value '{}'", val),
                            })?)
                        }
                        "data" => {
                            b.data = Some(val.parse::<u16>().map_err(|_| PanteonError::ProtocolDetailed {
                                context: "ShadowEntry",
                                detail: format!("invalid data value '{}'", val),
                            })?)
                        }
                        "written_at" => {
                            let clean = strip_quotes(val);
                            let ts = clean.parse::<u64>().map_err(|_| PanteonError::ProtocolDetailed {
                                context: "ShadowEntry",
                                detail: format!("invalid written_at value '{}'", val),
                            })?;
                            b.written_at = Some(ts);
                        }
                        _ => {} // unknown key in entry ignored for compatibility
                    }
                }
            }
        }
    }

    if let Some(builder) = current_entry.take() {
        entries.push(builder.build()?);
    }

    Ok(ShadowLedger {
        device_hint,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::to_toml_string;

    #[test]
    fn ledger_toml_roundtrip() {
        let ledger = ShadowLedger {
            device_hint: "/dev/hidraw1".to_string(),
            entries: vec![
                ShadowEntry::new(6, 24, 1792, 1712345678u64),
                ShadowEntry::new(0, 24, 1818, 1712345680u64),
                ShadowEntry::new(1, 10, 0x0055, 1712345690u64),
            ],
        };

        let toml_str = to_toml_string(&ledger);
        assert!(toml_str.contains("device_hint = \"/dev/hidraw1\""));
        assert!(toml_str.contains("bank = 6"));
        assert!(toml_str.contains("addr = 24"));
        assert!(toml_str.contains("data = 1792"));
        assert!(toml_str.contains("written_at = 1712345678\n"));

        let parsed = parse_ledger(&toml_str).expect("Failed to parse ledger TOML");
        assert_eq!(ledger, parsed);
    }

    #[test]
    fn parse_legacy_ledger_toml() {
        let legacy = r#"
device_hint = "/dev/hidraw1"

[[entries]]
bank = 6
addr = 24
data = 1792
written_at = "1712345678"

[[entries]]
bank = 0
addr = 24
data = 1818
written_at = "1712345680"
"#;
        let parsed = parse_ledger(legacy).expect("Failed to parse legacy ledger TOML");
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries[0].written_at, 1712345678);
        assert_eq!(parsed.entries[1].written_at, 1712345680);
    }

    #[test]
    fn parse_legacy_ledger_fixture() {
        let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("legacy_ledger.toml");
        let content = std::fs::read_to_string(fixture_path).expect("read legacy ledger fixture");
        let parsed = parse_ledger(&content).expect("parse legacy fixture");
        assert_eq!(parsed.device_hint, "/dev/hidraw1");
        assert_eq!(parsed.entries.len(), 3);
        assert_eq!(parsed.entries[0].bank, 6);
        assert_eq!(parsed.entries[0].written_at, 1712345678);
        assert_eq!(parsed.entries[1].written_at, 1712345680);
        assert_eq!(parsed.entries[2].written_at, 1712345690);
    }

    #[test]
    fn parse_ledger_truncated_fails() {
        let truncated = r#"
device_hint = "/dev/hidraw1"

[[entries]]
bank = 6
addr = 24
# data is missing
written_at = "1712345678"
"#;
        let err = parse_ledger(truncated).unwrap_err();
        assert_eq!(
            err,
            PanteonError::ProtocolDetailed {
                context: "ShadowEntry",
                detail: "missing required field 'data'".to_string(),
            }
        );
    }

    #[test]
    fn parse_ledger_garbage_value_fails() {
        let garbage_bank = r#"
device_hint = "/dev/hidraw1"

[[entries]]
bank = "garbage"
addr = 24
data = 1792
written_at = "1712345678"
"#;
        let err = parse_ledger(garbage_bank).unwrap_err();
        assert!(matches!(err, PanteonError::ProtocolDetailed { context: "ShadowEntry", .. }));

        let garbage_data = r#"
device_hint = "/dev/hidraw1"

[[entries]]
bank = 6
addr = 24
data = "not_a_number"
written_at = "1712345678"
"#;
        let err2 = parse_ledger(garbage_data).unwrap_err();
        assert!(matches!(err2, PanteonError::ProtocolDetailed { context: "ShadowEntry", .. }));
    }

    #[test]
    fn parse_ledger_unknown_section_fails() {
        let unknown = r#"
device_hint = "/dev/hidraw1"

[[invalid_section]]
foo = "bar"
"#;
        let err = parse_ledger(unknown).unwrap_err();
        assert_eq!(
            err,
            PanteonError::ProtocolDetailed {
                context: "ShadowLedger",
                detail: "unknown section '[[invalid_section]]'".to_string(),
            }
        );
    }
}
