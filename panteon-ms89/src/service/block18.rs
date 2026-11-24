//! Resolver for block 0x0018 (Device parameters: Active DPI slot, Fire delay, LED, Polling rate).
//!
//! Block 0x0018 is an 8-bank multi-bank register block. Writing individual banks via per-plan
//! commit causes the controller to latch/finalize the entire block, destroying neighbor values.
//! This resolver constructs a complete 8-bank `BlockWrite` by combining overrides, the shadow
//! ledger state, and factory reset snapshot defaults (ADR-003 §7, §8).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::device::BlockWrite;
use crate::model::ShadowLedger;
use crate::PanteonError;

/// Extracts factory reset fallback values for all 8 banks of block 0x0018 from `FACTORY_RESET_SNAPSHOT_TOML`.
pub fn get_factory_reset_block18() -> [Option<u16>; 8] {
    static CACHE: OnceLock<[Option<u16>; 8]> = OnceLock::new();
    *CACHE.get_or_init(|| {
        let mut banks = [None; 8];
        if let Ok(snap) = crate::service::load_factory_reset_snapshot() {
            for frame in &snap.frames {
                if frame[0] == 0x07 && frame[1] == 0x18 && frame[2] == 0x03 {
                    let bank = frame[3];
                    let addr = u16::from_le_bytes([frame[4], frame[5]]);
                    if addr == 0x0018 && (bank as usize) < 8 {
                        banks[bank as usize] = Some(u16::from_le_bytes([frame[6], frame[7]]));
                    }
                }
            }
        }
        banks
    })
}

/// Internal resolver allowing explicit fallback bank specification (used for tests and custom baselines).
pub fn resolve_block18_internal(
    ledger: &ShadowLedger,
    overrides: &[(u8, u16)],
    reset_banks: &[Option<u16>; 8],
) -> Result<BlockWrite, PanteonError> {
    let mut banks = [0u16; 8];

    // Collect overrides into a map (last override for a given bank takes precedence)
    let mut override_map = HashMap::new();
    for &(bank, data) in overrides {
        override_map.insert(bank, data);
    }

    // Bank 0 (ACTIVE_DPI_SLOT):
    // Strict resolution order:
    // 1. user override
    // 2. ShadowLedger
    // 3. PanteonError::Protocol("active DPI slot unknown, supply --active-slot / set slot in GUI")
    // (Never falls back to snapshot and never falls back to 0x0000).
    if let Some(&data) = override_map.get(&0) {
        banks[0] = data;
    } else if let Some(entry) = ledger
        .entries
        .iter()
        .rev()
        .find(|e| e.bank == 0 && e.addr == 0x0018)
    {
        banks[0] = entry.data;
    } else {
        return Err(PanteonError::Protocol(
            "active DPI slot unknown, supply --active-slot / set slot in GUI",
        ));
    }

    // Banks 1..=7:
    // Order: user override -> ShadowLedger -> reset_banks snapshot -> 0x0000 with warning.
    for b in 1..8u8 {
        if let Some(&data) = override_map.get(&b) {
            banks[b as usize] = data;
        } else if let Some(entry) = ledger
            .entries
            .iter()
            .rev()
            .find(|e| e.bank == b && e.addr == 0x0018)
        {
            banks[b as usize] = entry.data;
        } else if let Some(val) = reset_banks[b as usize] {
            banks[b as usize] = val;
        } else {
            eprintln!(
                "WARNING: bank {} @0x0018 not found in ledger or reset snapshot; falling back to 0x0000",
                b
            );
            banks[b as usize] = 0x0000;
        }
    }

    Ok(BlockWrite::new(0x0018, banks))
}

/// Resolve 8-bank BlockWrite for addr 0x0018:
/// 1. Bank 0 (Active DPI Slot): user override -> `ledger` -> Err("active DPI slot unknown, supply --active-slot / set slot in GUI").
/// 2. Banks 1..7: user override -> latest value from `ledger` -> factory reset snapshot -> fallback to 0x0000 with warning.
pub fn resolve_block18(ledger: &ShadowLedger, overrides: &[(u8, u16)]) -> Result<BlockWrite, PanteonError> {
    resolve_block18_internal(ledger, overrides, &get_factory_reset_block18())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ShadowEntry;

    #[test]
    fn test_resolve_block18_from_ledger() {
        let mut ledger = ShadowLedger::new("/dev/hidraw1");
        // Populate all banks in ledger for addr 0x0018
        ledger.append(ShadowEntry::new(0, 0x0018, 0x0719, "1000")); // Slot 2
        ledger.append(ShadowEntry::new(1, 0x0018, 0x073f, "1001"));
        ledger.append(ShadowEntry::new(2, 0x0018, 0x0764, "1002")); // Fire delay low (100 ms)
        ledger.append(ShadowEntry::new(3, 0x0018, 0x0700, "1003")); // Fire delay high
        ledger.append(ShadowEntry::new(4, 0x0018, 0x0782, "1004")); // LED effect
        ledger.append(ShadowEntry::new(5, 0x0018, 0x07fe, "1005")); // LED palette
        ledger.append(ShadowEntry::new(6, 0x0018, 0x0707, "1006")); // 125 Hz
        ledger.append(ShadowEntry::new(7, 0x0018, 0x0700, "1007"));

        // Override bank 6 with 0x0701 (500 Hz)
        let block = resolve_block18(&ledger, &[(6, 0x0701)]).expect("resolve block18 should succeed");

        assert_eq!(block.addr, 0x0018);
        assert_eq!(block.banks[0], 0x0719);
        assert_eq!(block.banks[1], 0x073f);
        assert_eq!(block.banks[2], 0x0764);
        assert_eq!(block.banks[3], 0x0700);
        assert_eq!(block.banks[4], 0x0782);
        assert_eq!(block.banks[5], 0x07fe);
        assert_eq!(block.banks[6], 0x0701); // Overridden from ledger's 0x0707
        assert_eq!(block.banks[7], 0x0700);
    }

    #[test]
    fn test_resolve_block18_fallback_to_factory_reset_with_ledger_bank0() {
        let mut ledger = ShadowLedger::default();
        ledger.append(ShadowEntry::new(0, 0x0018, 0x0718, "1000"));
        let block = resolve_block18(&ledger, &[]).expect("resolve block18 should succeed");

        assert_eq!(block.addr, 0x0018);
        // Bank 0 comes from ledger
        assert_eq!(block.banks[0], 0x0718);
        // Expected values from docs/state_reset.toml for banks 1..7:
        assert_eq!(block.banks[1], 0x073f);
        assert_eq!(block.banks[2], 0x0732);
        assert_eq!(block.banks[3], 0x0700);
        assert_eq!(block.banks[4], 0x07f3);
        assert_eq!(block.banks[5], 0x07ff);
        assert_eq!(block.banks[6], 0x0701);
        assert_eq!(block.banks[7], 0x0700);
    }

    #[test]
    fn test_resolve_block18_unknown_bank0_returns_error() {
        let empty_ledger = ShadowLedger::default();
        let err = resolve_block18(&empty_ledger, &[]).unwrap_err();
        assert_eq!(
            err,
            PanteonError::Protocol("active DPI slot unknown, supply --active-slot / set slot in GUI")
        );

        let err_internal = resolve_block18_internal(&empty_ledger, &[], &[None; 8]).unwrap_err();
        assert_eq!(
            err_internal,
            PanteonError::Protocol("active DPI slot unknown, supply --active-slot / set slot in GUI")
        );
    }

    #[test]
    fn test_resolve_block18_user_override_wins() {
        let mut ledger = ShadowLedger::default();
        ledger.append(ShadowEntry::new(0, 0x0018, 0x0718, "1000")); // Slot 1 in ledger
        // User overrides bank 0 to slot 4 (0x071b)
        let block = resolve_block18(&ledger, &[(0, 0x071b)]).expect("resolve block18 should succeed");
        assert_eq!(block.banks[0], 0x071b);
    }
}
