use std::path::Path;

use crate::capture::parse_capture_feature_sets;
use crate::device::{Device, Transport};
use crate::model::{current_timestamp_secs, ShadowEntry, ShadowLedger, StateSnapshot};
use crate::store::{parse_snapshot, to_toml_string};
use crate::{PanteonError, FEATURE_REPORT_LEN, REPORT_ID};

/// Import capture from raw pcap/pcapng bytes
pub fn import_capture(
    bytes: &[u8],
    pid: u16,
    label: &str,
) -> Result<StateSnapshot, PanteonError> {
    parse_capture_feature_sets(bytes, pid, label)
}

/// Save state snapshot to file as TOML
pub fn save_snapshot_to_file(path: &Path, snapshot: &StateSnapshot) -> Result<(), PanteonError> {
    let toml_str = to_toml_string(snapshot);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| PanteonError::Io(e.to_string()))?;
        }
    }
    std::fs::write(path, toml_str).map_err(|e| PanteonError::Io(e.to_string()))
}

/// Load and validate state snapshot from TOML file
pub fn load_snapshot_from_file(path: &Path) -> Result<StateSnapshot, PanteonError> {
    let content = std::fs::read_to_string(path).map_err(|e| PanteonError::Io(e.to_string()))?;
    let snapshot = parse_snapshot(&content)?;
    snapshot.validate()?;
    Ok(snapshot)
}

/// Embedded factory reset snapshot TOML (`docs/state_reset.toml`)
pub const FACTORY_RESET_SNAPSHOT_TOML: &str = include_str!("../../../docs/state_reset.toml");

/// Load embedded factory reset snapshot
pub fn load_factory_reset_snapshot() -> Result<StateSnapshot, PanteonError> {
    let snapshot = parse_snapshot(FACTORY_RESET_SNAPSHOT_TOML)?;
    snapshot.validate()?;
    Ok(snapshot)
}

/// Synthesize a full 867-frame Windows-equivalent Apply snapshot by overlaying
/// all confirmed ledger state entries onto the baseline factory reset snapshot.
pub fn build_full_push_snapshot(ledger: &ShadowLedger) -> Result<StateSnapshot, PanteonError> {
    let mut snapshot = load_factory_reset_snapshot()?;
    snapshot.label = "full-push".to_string();

    // Map each (bank, addr) to its latest data value in ledger
    let mut overrides: std::collections::HashMap<(u8, u16), u16> = std::collections::HashMap::new();
    for entry in &ledger.entries {
        overrides.insert((entry.bank, entry.addr), entry.data);
    }

    let mut applied_keys: std::collections::HashSet<(u8, u16)> = std::collections::HashSet::new();

    for frame in &mut snapshot.frames {
        if frame[0] == 0x07 && frame[1] == 0x18 && frame[2] == 0x03 {
            let bank = frame[3];
            let addr = u16::from_le_bytes([frame[4], frame[5]]);

            if let Some(&data) = overrides.get(&(bank, addr)) {
                frame[6] = (data & 0xff) as u8;
                frame[7] = ((data >> 8) & 0xff) as u8;
                applied_keys.insert((bank, addr));
            }
        }
    }

    // Check for ledger entries that do not match any baseline frame
    for &(bank, addr) in overrides.keys() {
        if !applied_keys.contains(&(bank, addr)) {
            eprintln!(
                "WARNING: ledger entry for bank {} addr 0x{:04x} not found in baseline snapshot; ignoring",
                bank, addr
            );
        }
    }

    Ok(snapshot)
}


/// Single item in a state restoration sequence
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePlanItem {
    pub index: usize,
    pub frame: [u8; FEATURE_REPORT_LEN],
    pub delay_ms: u64,
}

/// Build plan items for restoring state snapshot
pub fn build_restore_plan(
    snapshot: &StateSnapshot,
    settle_override: Option<u64>,
) -> Vec<RestorePlanItem> {
    snapshot
        .frames
        .iter()
        .zip(snapshot.delays_ms.iter())
        .enumerate()
        .map(|(index, (&frame, &default_delay))| RestorePlanItem {
            index,
            frame,
            delay_ms: settle_override.unwrap_or(default_delay as u64),
        })
        .collect()
}

/// Restore a snapshot onto the target device with wire verification and ledger tracking
pub fn restore_snapshot(
    dev: &mut Device,
    snapshot: &StateSnapshot,
    settle_override: Option<u64>,
    force_pid_mismatch: bool,
    ledger: &mut Option<&mut ShadowLedger>,
) -> Result<(), PanteonError> {
    if !dev.identity().protocol_verified {
        return Err(PanteonError::ProtocolDetailed {
            context: "restore_state",
            detail: format!(
                "protocol unverified for PID 0x{:04x} — run verify-protocol first",
                dev.identity().pid
            ),
        });
    }

    if snapshot.source_pid != dev.identity().pid && !force_pid_mismatch {
        return Err(PanteonError::ProtocolDetailed {
            context: "restore_state",
            detail: format!(
                "PID mismatch: snapshot source PID is 0x{:04x}, device PID is 0x{:04x}. Pass --force-pid-mismatch to override.",
                snapshot.source_pid, dev.identity().pid
            ),
        });
    }

    let plan = build_restore_plan(snapshot, settle_override);

    for item in plan {
        let hex_str = item.frame.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");

        // 1. Send frame via raw_write_feature
        dev.raw_write_feature(&item.frame).map_err(|e| PanteonError::ProtocolDetailed {
            context: "restore_state",
            detail: format!("frame [{}] {} write failed: {}", item.index, hex_str, e),
        })?;

        // 2. Read feature ACK check
        let mut ack = [0u8; FEATURE_REPORT_LEN];
        ack[0] = REPORT_ID;
        dev.read_feature(&mut ack).map_err(|e| PanteonError::ProtocolDetailed {
            context: "restore_state",
            detail: format!("frame [{}] {} ACK read failed: {}", item.index, hex_str, e),
        })?;

        if ack[0] != REPORT_ID {
            return Err(PanteonError::ProtocolDetailed {
                context: "restore_state",
                detail: format!(
                    "frame [{}] {} ACK mismatch: expected report_id 0x{:02x}, got 0x{:02x}",
                    item.index, hex_str, REPORT_ID, ack[0]
                ),
            });
        }

        // 3. Record in shadow ledger if 0x18 data write
        if item.frame[1] == 0x18 && item.frame[2] == 0x03 {
            if let Some(ref mut led) = ledger {
                let bank = item.frame[3];
                let addr = u16::from_le_bytes([item.frame[4], item.frame[5]]);
                let data = u16::from_le_bytes([item.frame[6], item.frame[7]]);
                led.append(ShadowEntry::new(bank, addr, data, current_timestamp_secs()));
            }
        }

        // 4. Delay
        if item.delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(item.delay_ms));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_factory_reset_snapshot() {
        let snap = load_factory_reset_snapshot().expect("Embedded snapshot should parse");
        assert_eq!(snap.frames.len(), 867);
        assert_eq!(snap.delays_ms.len(), 867);
        assert_eq!(snap.source_pid, 0x1440);
        assert_eq!(snap.label, "windows-reset");

        let plan = build_restore_plan(&snap, None);
        assert_eq!(plan.len(), 867);
        assert_eq!(plan[0].frame, [0x07, 0x18, 0x03, 0x00, 0x01, 0x00, 0x03, 0x00]);
        assert_eq!(plan[866].frame, [0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_build_full_push_snapshot_overlays_ledger() {
        let mut ledger = ShadowLedger::default();
        // Overlay 500 Hz polling rate (0x0701 on Bank 6, Addr 0x0018)
        ledger.append(ShadowEntry::new(6, 0x0018, 0x0701, "100"));

        let full_push = build_full_push_snapshot(&ledger).expect("full push build");
        assert_eq!(full_push.frames.len(), 867);
        assert_eq!(full_push.label, "full-push");

        // Verify that Bank 6 Addr 0x0018 frames now have 0x0701 instead of factory default 0x0700
        let modified_frames: Vec<_> = full_push
            .frames
            .iter()
            .filter(|f| f[1] == 0x18 && f[2] == 0x03 && f[3] == 6 && f[4] == 0x18 && f[5] == 0x00)
            .collect();
        assert_eq!(modified_frames.len(), 1);
        assert_eq!(modified_frames[0][6], 0x01);
        assert_eq!(modified_frames[0][7], 0x07);
    }

    #[test]
    fn test_build_full_push_snapshot_warning_on_unmatched_ledger_entry() {
        let mut ledger = ShadowLedger::default();
        // Add a valid entry
        ledger.append(ShadowEntry::new(6, 0x0018, 0x0701, "100"));
        // Add an invalid/missing entry that does not exist in state_reset.toml baseline
        ledger.append(ShadowEntry::new(99, 0x9999, 0xbeef, "101"));

        let full_push = build_full_push_snapshot(&ledger).expect("full push build should succeed without error");
        assert_eq!(full_push.frames.len(), 867);

        // Verify valid entry is applied
        let modified_frames: Vec<_> = full_push
            .frames
            .iter()
            .filter(|f| f[1] == 0x18 && f[2] == 0x03 && f[3] == 6 && f[4] == 0x18 && f[5] == 0x00)
            .collect();
        assert_eq!(modified_frames.len(), 1);
        assert_eq!(modified_frames[0][6], 0x01);
        assert_eq!(modified_frames[0][7], 0x07);

        // Verify missing entry was ignored and no frame has bank 99 or addr 0x9999
        let unmatched_frames: Vec<_> = full_push
            .frames
            .iter()
            .filter(|f| f[3] == 99 || (u16::from_le_bytes([f[4], f[5]]) == 0x9999))
            .collect();
        assert_eq!(unmatched_frames.len(), 0);
    }
}

