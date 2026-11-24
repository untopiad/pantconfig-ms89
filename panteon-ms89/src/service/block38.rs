//! Resolver for block 0x0038 (Color matrix block 4: Effect parameters & global brightness).
//!
//! Block 0x0038 is an 8-bank multi-bank register block.
//! Banks 0..6 contain confirmed fixed effect parameters (bank 2 = 0x070a, bank 6 = 0x078f).
//! Bank 7 controls global LED brightness (levels 0..=8 encoded active-low: `0x0700 | (((8 - level) as u16) << 4)`).
//!
//! Bank 7 resolution order (R8):
//! 1. user override (`brightness_override: Some(0..=8)`)
//! 2. ShadowLedger (entry for bank 7 @ 0x0038)
//! 3. PanteonError::Protocol("LED brightness unknown, supply --brightness / set in GUI")
//! (No silent fallback to snapshot or 0x0700/0x0000).

use crate::device::BlockWrite;
use crate::model::ShadowLedger;
use crate::PanteonError;

/// Resolve block 0x0038 (8 banks).
///
/// Banks 0..6: Fixed effect parameters:
/// - Bank 0: 0x0700
/// - Bank 1: 0x0700
/// - Bank 2: 0x070a
/// - Bank 3: 0x0700
/// - Bank 4: 0x0700
/// - Bank 5: 0x0700
/// - Bank 6: 0x078f
///
/// Bank 7 (LED_BRIGHTNESS):
/// Strict resolution order:
/// 1. user override (level 0..=8 -> 0x0700 | (((8 - level) as u16) << 4))
/// 2. ShadowLedger (entry for bank 7 @ 0x0038)
/// 3. PanteonError::Protocol("LED brightness unknown, supply --brightness / set in GUI")
/// (No silent fallback to snapshot or 0x0700/0x0000).
pub fn resolve_block38(
    ledger: &ShadowLedger,
    brightness_override: Option<u8>,
) -> Result<BlockWrite, PanteonError> {
    let bank7 = if let Some(lvl) = brightness_override {
        crate::codec::brightness_to_bank7(lvl)?
    } else if let Some(entry) = ledger
        .entries
        .iter()
        .rev()
        .find(|e| e.bank == 7 && e.addr == 0x0038)
    {
        entry.data
    } else {
        return Err(PanteonError::Protocol(
            "LED brightness unknown, supply --brightness / set in GUI",
        ));
    };

    let banks = [
        0x0700, 0x0700, 0x070a, 0x0700, 0x0700, 0x0700, 0x078f, bank7,
    ];
    Ok(BlockWrite::new(0x0038, banks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ShadowEntry;

    #[test]
    fn test_resolve_block38_with_override() {
        let ledger = ShadowLedger::default();
        let b = resolve_block38(&ledger, Some(2)).unwrap();
        assert_eq!(b.addr, 0x0038);
        assert_eq!(b.banks[2], 0x070a);
        assert_eq!(b.banks[6], 0x078f);
        assert_eq!(b.banks[7], 0x0760); // 25% -> 0x0760
    }

    #[test]
    fn test_resolve_block38_from_ledger() {
        let mut ledger = ShadowLedger::default();
        ledger.append(ShadowEntry::new(7, 0x0038, 0x0740, "1000")); // 50%
        let b = resolve_block38(&ledger, None).unwrap();
        assert_eq!(b.banks[7], 0x0740);
    }

    #[test]
    fn test_resolve_block38_override_wins_over_ledger() {
        let mut ledger = ShadowLedger::default();
        ledger.append(ShadowEntry::new(7, 0x0038, 0x0740, "1000")); // 50% in ledger
        let b = resolve_block38(&ledger, Some(8)).unwrap(); // 100% override
        assert_eq!(b.banks[7], 0x0700);
    }

    #[test]
    fn test_resolve_block38_empty_ledger_no_override_fails() {
        let ledger = ShadowLedger::default();
        let res = resolve_block38(&ledger, None);
        assert!(res.is_err());
        match res.unwrap_err() {
            PanteonError::Protocol(msg) => {
                assert_eq!(msg, "LED brightness unknown, supply --brightness / set in GUI");
            }
            other => panic!("Expected Protocol error, got {:?}", other),
        }
    }
}
