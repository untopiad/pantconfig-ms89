use std::collections::BTreeMap;
use std::path::PathBuf;

use panteon_ms89::codec::{decode_descriptor_page, DescriptorPage};
use panteon_ms89::device::{select_device_path, Device, WriteGateLevel};
use panteon_ms89::model::{registers, DeviceState, PollingRate, ShadowLedger};
use panteon_ms89::PanteonError;

use super::definition::GateArgs;
use panteon_ms89::service::{load_ledger, resolve_state_path};

/// Pure formatting function for dump-state output
pub fn format_state_output(ledger: &ShadowLedger, descriptor: Option<&DescriptorPage>) -> String {
    let mut out = String::new();

    // 1. [SHADOW LEDGER]
    out.push_str(&format!(
        "[SHADOW LEDGER] (наши записи, {} последних)\n",
        ledger.entries.len()
    ));
    if ledger.entries.is_empty() {
        out.push_str("  ledger empty: no writes recorded by this tool\n");
    } else {
        out.push_str(&format!("  Device hint: {}\n", ledger.device_hint));
        out.push_str("  Bank  Addr    Data    Written At\n");
        for entry in &ledger.entries {
            out.push_str(&format!(
                "  {:<4}  0x{:04x}  0x{:04x}  {}\n",
                entry.bank, entry.addr, entry.data, entry.written_at
            ));
        }
    }

    // 2. [LIVE DESCRIPTOR]
    out.push_str("\n[LIVE DESCRIPTOR]");
    if let Some(desc) = descriptor {
        out.push_str(" (Level 1 read)\n");
        let bank_flags_str = desc
            .bank_flags
            .iter()
            .map(|b| format!("0x{:02x}", b))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "  unlock_state=0x{:02x}, PID=0x{:04x}, vid_lo=0x{:02x} (static), write_echo=0x{:02x} (last write data_lo), bank-ID 8..15=[{}]\n",
            desc.unlock_state, desc.pid, desc.vid_lo, desc.write_echo, bank_flags_str
        ));
    } else {
        out.push_str(" (skipped, requires Level 1: --confirm --unsafe-addressing)\n");
    }

    // 3. [INTERPRETATION]
    out.push_str("\n[INTERPRETATION] (per-session)\n");
    if ledger.entries.is_empty() {
        out.push_str("  ledger empty: no shadow records to interpret\n");
    } else {
        // Collect latest value for each (bank, addr)
        let mut latest_map = BTreeMap::new();
        for entry in &ledger.entries {
            latest_map.insert((entry.bank, entry.addr), entry.data);
        }

        for ((bank, addr), data) in latest_map {
            if (bank, addr) == registers::POLLING_RATE {
                let val_str = if let Some(rate) = PollingRate::from_register_code(data) {
                    format!("0x{:04x} = {} Hz", data, rate.as_hz())
                } else {
                    format!("0x{:04x} (unknown code)", data)
                };
                out.push_str(&format!(
                    "  POLLING_RATE (bank {}, 0x{:04x}): {}   <- по ledger (shadow)\n",
                    bank, addr, val_str
                ));
            } else if (bank, addr) == registers::ACTIVE_DPI_SLOT {
                let val_str = if let Ok(slot) = DeviceState::raw_to_slot_index(data) {
                    format!("0x{:04x} = slot {}", data, slot)
                } else {
                    format!("0x{:04x} (unknown slot)", data)
                };
                out.push_str(&format!(
                    "  ACTIVE_DPI_SLOT (bank {}, 0x{:04x}): {}   <- по ledger (shadow)\n",
                    bank, addr, val_str
                ));
            } else {
                out.push_str(&format!(
                    "  REGISTER (bank {}, 0x{:04x}): 0x{:04x}   <- по ledger (shadow)\n",
                    bank, addr, data
                ));
            }
        }
    }

    out
}

pub fn run_dump_state(
    device: Option<PathBuf>,
    state: Option<PathBuf>,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let state_path = resolve_state_path(state);
    let ledger = load_ledger(&state_path).unwrap_or_default();

    let descriptor = if gate.authorized(WriteGateLevel::Level1AddressingOnly) {
        let target_path = select_device_path(device)?;
        println!(
            "[Level 1 Authorized] Opening {} under Write-Gate Level 1 (Addressing Only)...",
            target_path.display()
        );
        let mut dev =
            Device::open_with_gate_level(&target_path, WriteGateLevel::Level1AddressingOnly)?;
        let raw_bytes = dev.experimental_read(0, 0, 32, Device::READ_PROTOCOL_DELAY)?;
        Some(decode_descriptor_page(&raw_bytes)?)
    } else {
        None
    };

    let output = format_state_output(&ledger, descriptor.as_ref());
    print!("{}", output);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use panteon_ms89::model::ShadowEntry;

    #[test]
    fn test_format_state_output_golden() {
        use panteon_ms89::codec::LIVE_DESCRIPTOR_PAGE_BYTES;

        let mut ledger = ShadowLedger::new("/dev/hidraw1");
        ledger.append(ShadowEntry::new(6, 0x0018, 0x0700, "1712345678"));
        ledger.append(ShadowEntry::new(0, 0x0018, 0x071a, "1712345680"));
        ledger.append(ShadowEntry::new(2, 0x0010, 0x0042, "1712345690"));

        let desc = decode_descriptor_page(&LIVE_DESCRIPTOR_PAGE_BYTES).unwrap();

        let formatted = format_state_output(&ledger, Some(&desc));
        assert!(formatted.contains("[SHADOW LEDGER] (наши записи, 3 последних)"));
        assert!(formatted.contains("Device hint: /dev/hidraw1"));
        assert!(formatted.contains("6     0x0018  0x0700  1712345678"));
        assert!(formatted.contains("[LIVE DESCRIPTOR] (Level 1 read)"));
        assert!(formatted.contains("unlock_state=0x03, PID=0x1440, vid_lo=0xfa (static), write_echo=0x30 (last write data_lo)"));
        assert!(formatted.contains("[INTERPRETATION]"));
        assert!(formatted.contains("POLLING_RATE (bank 6, 0x0018): 0x0700 = 1000 Hz   <- по ledger (shadow)"));
        assert!(formatted.contains("ACTIVE_DPI_SLOT (bank 0, 0x0018): 0x071a = slot 3   <- по ledger (shadow)"));
        assert!(formatted.contains("REGISTER (bank 2, 0x0010): 0x0042   <- по ledger (shadow)"));
    }

    #[test]
    fn test_format_state_output_empty_ledger_no_descriptor() {
        let empty_ledger = ShadowLedger::default();
        let formatted = format_state_output(&empty_ledger, None);

        assert!(formatted.contains("ledger empty: no writes recorded by this tool"));
        assert!(formatted.contains("[LIVE DESCRIPTOR] (skipped, requires Level 1: --confirm --unsafe-addressing)"));
        assert!(formatted.contains("ledger empty: no shadow records to interpret"));
    }

    #[test]
    fn test_format_state_output_post_write_descriptor() {
        use panteon_ms89::codec::POST_WRITE_DESCRIPTOR_PAGE_BYTES;

        let empty_ledger = ShadowLedger::default();
        let desc = decode_descriptor_page(&POST_WRITE_DESCRIPTOR_PAGE_BYTES).unwrap();
        let formatted = format_state_output(&empty_ledger, Some(&desc));

        assert!(formatted.contains("unlock_state=0x03, PID=0x1440, vid_lo=0xfa (static), write_echo=0x00 (last write data_lo)"));
        assert!(!formatted.contains("VID=0x30fa"));
    }
}
