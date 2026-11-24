pub mod device;
pub mod ledger;
pub mod profile;
pub mod registers;
pub mod snapshot;

pub use device::*;
pub use ledger::*;
pub use profile::*;
pub use registers::*;
pub use snapshot::*;

/// Returns current system timestamp in unix seconds as a string
pub fn current_timestamp_secs() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

/// Returns current system timestamp in unix seconds as u64
pub fn current_unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PanteonError;

    #[test]
    fn test_state_snapshot_validation() {
        let frames = vec![[0x07, 0x18, 0x03, 0x00, 0x01, 0x00, 0x03, 0x00]];
        let delays = vec![0u16];
        let snapshot = StateSnapshot::new(0x1440, "1712345678", "test", frames.clone(), delays).unwrap();
        assert_eq!(snapshot.source_pid, 0x1440);
        assert_eq!(snapshot.frames.len(), 1);
        assert_eq!(snapshot.delays_ms.len(), 1);

        // Mismatched delays count must fail
        let invalid = StateSnapshot::new(0x1440, "1712345678", "test", frames, vec![0u16, 10u16]);
        assert!(invalid.is_err());
        match invalid.unwrap_err() {
            PanteonError::ProtocolDetailed { context, detail } => {
                assert_eq!(context, "StateSnapshot::validate");
                assert!(detail.contains("frames count (1) does not match delays_ms count (2)"));
            }
            err => panic!("Unexpected error: {:?}", err),
        }
    }

    #[test]
    fn test_shadow_entry_and_ledger() {
        let mut ledger = ShadowLedger::new("/dev/hidraw1");
        assert_eq!(ledger.device_hint, "/dev/hidraw1");
        assert!(ledger.entries.is_empty());

        let entry = ShadowEntry::new(6, 24, 0x0700, "1712345678");
        ledger.append(entry.clone());

        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(ledger.entries[0], entry);
        assert_eq!(ledger.entries[0].bank, 6);
        assert_eq!(ledger.entries[0].addr, 24);
        assert_eq!(ledger.entries[0].data, 1792);
        assert_eq!(ledger.entries[0].written_at, 1712345678);
    }

    #[test]
    fn test_current_timestamp_secs() {
        let ts = current_timestamp_secs();
        let secs = ts.parse::<u64>().expect("timestamp should be integer seconds");
        assert!(secs > 1700000000);
    }
}
