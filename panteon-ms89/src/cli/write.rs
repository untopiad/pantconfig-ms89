use std::path::PathBuf;

use panteon_ms89::device::WriteGateLevel;
use panteon_ms89::model::{registers, DeviceState, PollingRate};
use panteon_ms89::service::{
    execute_block_session, open_device, plan_block_session_frames, plan_dpi_slot, plan_fire_delay,
    plan_polling_rate, record_write, resolve_block18,
};
pub use panteon_ms89::service::{
    append_entry, load_ledger, resolve_state_path, save_ledger_atomic,
};
use panteon_ms89::PanteonError;

use super::args::{authorize_device_write, print_unverified_dry_run_warning_if_needed, GateArgs};
use super::render::print_gate_authorized;

#[allow(clippy::too_many_arguments)]
pub fn run_set_polling(
    rate: u16,
    device: Option<PathBuf>,
    dry_run: bool,
    settle_ms: u64,
    commit_flash: bool,
    get_first: bool,
    state: Option<PathBuf>,
    no_ledger: bool,
    active_slot: Option<u8>,
    no_soft_reset: bool,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let _plan = plan_polling_rate(rate)?;
    let polling_rate = PollingRate::from_hz(rate).unwrap();
    let code = polling_rate.to_register_code();

    let state_path = resolve_state_path(state);
    let ledger = load_ledger(&state_path).unwrap_or_default();
    let mut overrides = vec![(registers::POLLING_RATE.0, code)];
    if let Some(s) = active_slot {
        if !(1..=6).contains(&s) {
            return Err(PanteonError::ProtocolDetailed {
                context: "set-polling",
                detail: format!("active-slot must be 1..=6, got {}", s),
            });
        }
        let raw_slot = DeviceState::slot_index_to_raw(s)?;
        overrides.push((registers::ACTIVE_DPI_SLOT.0, raw_slot));
    }
    let block18 = resolve_block18(&ledger, &overrides)?;
    let session_frames = plan_block_session_frames(&[block18.clone()], None, commit_flash);

    println!(
        "[DRY-RUN] Planned Block Session (1 blocks, {} frames total) for POLLING_RATE ({} Hz, active slot Bank 0: 0x{:04x}):",
        session_frames.len(),
        rate,
        block18.banks[0]
    );
    if get_first {
        println!("  Pre-sequence: GET_REPORT Feature 0x07 handshake will be issued prior to Phase P1.");
    }
    if settle_ms > 0 {
        println!("  Timing delay: {} ms pause between frames.", settle_ms);
    }
    for (idx, f) in session_frames.iter().enumerate() {
        let hex = f.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
        println!("  Frame {:04}: [{}]", idx, hex);
    }
    if commit_flash {
        println!("  Commit tail: 5 frames (including mode 0x10 Flash Flush)");
    } else {
        println!("  Commit tail: 4 frames (Soft-Apply without Flash Flush)");
    }

    if gate.authorized(WriteGateLevel::Level2ConfigurationWrite) && !dry_run {
        let (target_path, mut dev) = open_device(device, WriteGateLevel::Level2ConfigurationWrite)?;
        authorize_device_write(&mut dev, &gate)?;

        print_gate_authorized(WriteGateLevel::Level2ConfigurationWrite, &target_path);

        if get_first {
            println!("[GET_FIRST] Performing GET_REPORT handshake prior to write sequence...");
            let state = panteon_ms89::service::read_handshake(&mut dev)?;
            if let Some(hs) = state.handshake {
                println!(
                    "  Handshake response: signature=0x{:04x}, revision=0x{:02x}, status=0x{:02x}, flags=0x{:02x}",
                    hs.signature, hs.revision, hs.status, hs.flags
                );
            }
            if settle_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(settle_ms));
            }
        }

        println!(
            "Executing block session write transaction (settle_ms = {}, commit_flash = {})...",
            settle_ms, commit_flash
        );
        match execute_block_session(&mut dev, &[block18], false, commit_flash, settle_ms) {
            Ok(()) => {
                println!(
                    "Transaction applied successfully via block session. Status: unverified (hardware readback unavailable, ACK verified)."
                );
                if !no_ledger {
                    record_write(
                        &state_path,
                        &target_path,
                        registers::POLLING_RATE.0,
                        registers::POLLING_RATE.1,
                        code,
                    )?;
                    if let Some(s) = active_slot {
                        let raw_slot = DeviceState::slot_index_to_raw(s)?;
                        record_write(
                            &state_path,
                            &target_path,
                            registers::ACTIVE_DPI_SLOT.0,
                            registers::ACTIVE_DPI_SLOT.1,
                            raw_slot,
                        )?;
                    }
                    println!("Recorded write to shadow ledger: {}", state_path.display());
                }
                let bus_dev = panteon_ms89::service::discover().ok().and_then(|devs| {
                    devs.into_iter().find(|d| d.path == target_path).and_then(|d| match (d.usb_bus, d.usb_dev) {
                        (Some(b), Some(d_num)) => Some((b, d_num)),
                        _ => None,
                    })
                });
                if no_soft_reset {
                    println!("USB soft-reset skipped (--no-soft-reset): device stays enumerated; settings apply via Soft-Apply tail (R10.1 live-apply experiment).");
                } else if let Some((b, d)) = bus_dev {
                    let _ = panteon_ms89::service::soft_reset((b, d));
                    println!("USB soft-reset triggered for /dev/bus/usb/{:03}/{:03}", b, d);
                }
            }
            Err(e) => {
                eprintln!("Transaction error: {}", e);
                return Err(e);
            }
        }
    } else {
        if let Ok((_, dev)) = open_device(device, WriteGateLevel::Level0ReadOnly) {
            print_unverified_dry_run_warning_if_needed(&dev);
        }
        println!(
            "\nNotice: Dry-run active by default. No data written to hardware."
        );
        println!(
            "To apply configuration, pass Level 2 authorization flags:\n  --write --confirm"
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn run_set_active_dpi(
    slot: u8,
    device: Option<PathBuf>,
    dry_run: bool,
    settle_ms: u64,
    commit_flash: bool,
    state: Option<PathBuf>,
    no_ledger: bool,
    no_soft_reset: bool,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let _plan = plan_dpi_slot(slot)?;
    let raw_val = DeviceState::slot_index_to_raw(slot)?;

    let state_path = resolve_state_path(state);
    let ledger = load_ledger(&state_path).unwrap_or_default();
    let block18 = resolve_block18(&ledger, &[(registers::ACTIVE_DPI_SLOT.0, raw_val)])?;
    let session_frames = plan_block_session_frames(&[block18.clone()], None, commit_flash);

    println!(
        "[DRY-RUN] Planned Block Session (1 blocks, {} frames total) for ACTIVE_DPI_SLOT (Slot {} -> raw 0x{:04x}):",
        session_frames.len(),
        slot,
        raw_val
    );
    if settle_ms > 0 {
        println!("  Timing delay: {} ms pause between frames.", settle_ms);
    }
    for (idx, f) in session_frames.iter().enumerate() {
        let hex = f.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
        println!("  Frame {:04}: [{}]", idx, hex);
    }
    if commit_flash {
        println!("  Commit tail: 5 frames (including mode 0x10 Flash Flush)");
    } else {
        println!("  Commit tail: 4 frames (Soft-Apply without Flash Flush)");
    }

    if gate.authorized(WriteGateLevel::Level2ConfigurationWrite) && !dry_run {
        let (target_path, mut dev) = open_device(device, WriteGateLevel::Level2ConfigurationWrite)?;
        authorize_device_write(&mut dev, &gate)?;

        print_gate_authorized(WriteGateLevel::Level2ConfigurationWrite, &target_path);

        println!(
            "Executing block session write transaction (settle_ms = {}, commit_flash = {})...",
            settle_ms, commit_flash
        );
        match execute_block_session(&mut dev, &[block18], false, commit_flash, settle_ms) {
            Ok(()) => {
                println!(
                    "Active DPI slot applied successfully via block session. Status: unverified (hardware readback unavailable, ACK verified)."
                );
                if !no_ledger {
                    record_write(
                        &state_path,
                        &target_path,
                        registers::ACTIVE_DPI_SLOT.0,
                        registers::ACTIVE_DPI_SLOT.1,
                        raw_val,
                    )?;
                    println!("Recorded write to shadow ledger: {}", state_path.display());
                }
                let bus_dev = panteon_ms89::service::discover().ok().and_then(|devs| {
                    devs.into_iter().find(|d| d.path == target_path).and_then(|d| match (d.usb_bus, d.usb_dev) {
                        (Some(b), Some(d_num)) => Some((b, d_num)),
                        _ => None,
                    })
                });
                if no_soft_reset {
                    println!("USB soft-reset skipped (--no-soft-reset): device stays enumerated; settings apply via Soft-Apply tail (R10.1 live-apply experiment).");
                } else if let Some((b, d)) = bus_dev {
                    let _ = panteon_ms89::service::soft_reset((b, d));
                    println!("USB soft-reset triggered for /dev/bus/usb/{:03}/{:03}", b, d);
                }
            }
            Err(e) => {
                eprintln!("Transaction error: {}", e);
                return Err(e);
            }
        }
    } else {
        if let Ok((_, dev)) = open_device(device, WriteGateLevel::Level0ReadOnly) {
            print_unverified_dry_run_warning_if_needed(&dev);
        }
        println!(
            "\nNotice: Dry-run active by default. No data written to hardware."
        );
        println!(
            "To apply configuration, pass Level 2 authorization flags:\n  --write --confirm"
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn run_set_fire_delay(
    delay_ms: u16,
    device: Option<PathBuf>,
    dry_run: bool,
    settle_ms: u64,
    commit_flash: bool,
    state: Option<PathBuf>,
    no_ledger: bool,
    active_slot: Option<u8>,
    no_soft_reset: bool,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let (p_low, p_high) = plan_fire_delay(delay_ms)?;

    let state_path = resolve_state_path(state);
    let ledger = load_ledger(&state_path).unwrap_or_default();
    let mut overrides = vec![
        (registers::FIRE_DELAY_LOW.0, p_low.data),
        (registers::FIRE_DELAY_HIGH.0, p_high.data),
    ];
    if let Some(s) = active_slot {
        if !(1..=6).contains(&s) {
            return Err(PanteonError::ProtocolDetailed {
                context: "set-fire-delay",
                detail: format!("active-slot must be 1..=6, got {}", s),
            });
        }
        let raw_slot = DeviceState::slot_index_to_raw(s)?;
        overrides.push((registers::ACTIVE_DPI_SLOT.0, raw_slot));
    }
    let block18 = resolve_block18(&ledger, &overrides)?;
    let session_frames = plan_block_session_frames(&[block18.clone()], None, commit_flash);

    println!(
        "[DRY-RUN] Planned Block Session (1 blocks, {} frames total) for FIRE_DELAY ({} ms -> Bank 2: 0x{:04x}, Bank 3: 0x{:04x}, active slot Bank 0: 0x{:04x}):",
        session_frames.len(),
        delay_ms,
        p_low.data,
        p_high.data,
        block18.banks[0]
    );
    if settle_ms > 0 {
        println!("  Timing delay: {} ms pause between frames.", settle_ms);
    }
    for (idx, f) in session_frames.iter().enumerate() {
        let hex = f.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
        println!("  Frame {:04}: [{}]", idx, hex);
    }
    if commit_flash {
        println!("  Commit tail: 5 frames (including mode 0x10 Flash Flush)");
    } else {
        println!("  Commit tail: 4 frames (Soft-Apply without Flash Flush)");
    }

    if gate.authorized(WriteGateLevel::Level2ConfigurationWrite) && !dry_run {
        let (target_path, mut dev) = open_device(device, WriteGateLevel::Level2ConfigurationWrite)?;
        authorize_device_write(&mut dev, &gate)?;

        print_gate_authorized(WriteGateLevel::Level2ConfigurationWrite, &target_path);

        println!(
            "Executing block session write transaction (settle_ms = {}, commit_flash = {})...",
            settle_ms, commit_flash
        );
        match execute_block_session(&mut dev, &[block18], false, commit_flash, settle_ms) {
            Ok(()) => {
                println!(
                    "Fire delay applied successfully via block session. Status: unverified (hardware readback unavailable, ACK verified)."
                );
                if !no_ledger {
                    record_write(
                        &state_path,
                        &target_path,
                        registers::FIRE_DELAY_LOW.0,
                        registers::FIRE_DELAY_LOW.1,
                        p_low.data,
                    )?;
                    record_write(
                        &state_path,
                        &target_path,
                        registers::FIRE_DELAY_HIGH.0,
                        registers::FIRE_DELAY_HIGH.1,
                        p_high.data,
                    )?;
                    if let Some(s) = active_slot {
                        let raw_slot = DeviceState::slot_index_to_raw(s)?;
                        record_write(
                            &state_path,
                            &target_path,
                            registers::ACTIVE_DPI_SLOT.0,
                            registers::ACTIVE_DPI_SLOT.1,
                            raw_slot,
                        )?;
                    }
                    println!("Recorded writes to shadow ledger: {}", state_path.display());
                }
                let bus_dev = panteon_ms89::service::discover().ok().and_then(|devs| {
                    devs.into_iter().find(|d| d.path == target_path).and_then(|d| match (d.usb_bus, d.usb_dev) {
                        (Some(b), Some(d_num)) => Some((b, d_num)),
                        _ => None,
                    })
                });
                if no_soft_reset {
                    println!("USB soft-reset skipped (--no-soft-reset): device stays enumerated; settings apply via Soft-Apply tail (R10.1 live-apply experiment).");
                } else if let Some((b, d)) = bus_dev {
                    let _ = panteon_ms89::service::soft_reset((b, d));
                    println!("USB soft-reset triggered for /dev/bus/usb/{:03}/{:03}", b, d);
                }
            }
            Err(e) => {
                eprintln!("Transaction error: {}", e);
                return Err(e);
            }
        }
    } else {
        if let Ok((_, dev)) = open_device(device, WriteGateLevel::Level0ReadOnly) {
            print_unverified_dry_run_warning_if_needed(&dev);
        }
        println!(
            "\nNotice: Dry-run active by default. No data written to hardware."
        );
        println!(
            "To apply configuration, pass Level 2 authorization flags:\n  --write --confirm"
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn run_set_led(
    brightness: Option<u8>,
    effect: Option<u8>,
    speed: Option<u8>,
    device: Option<PathBuf>,
    dry_run: bool,
    settle_ms: u64,
    commit_flash: bool,
    state: Option<PathBuf>,
    no_ledger: bool,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let state_path = resolve_state_path(state);
    let ledger = load_ledger(&state_path).unwrap_or_default();

    let mut blocks = Vec::new();
    let mut recorded_writes = Vec::new();

    // 1. Resolve block 0x0038 if brightness is supplied
    if let Some(b) = brightness {
        let block38 = panteon_ms89::service::resolve_block38(&ledger, Some(b))?;
        blocks.push(block38.clone());
        recorded_writes.push((7u8, 0x0038u16, block38.banks[7]));
    }

    // 2. Resolve block 0x0018 if effect or speed is supplied
    if effect.is_some() || speed.is_some() {
        let eff = effect.unwrap_or(0x0f);
        let spd = speed.unwrap_or(0x03);
        let plan = panteon_ms89::service::plan_led_effect_speed(eff, spd)?;
        let block18 = panteon_ms89::service::resolve_block18(&ledger, &[(plan.bank, plan.data)])?;
        blocks.push(block18.clone());
        recorded_writes.push((plan.bank, plan.addr, plan.data));
    }

    if blocks.is_empty() {
        let block38 = panteon_ms89::service::resolve_block38(&ledger, None)?;
        blocks.push(block38);
    }

    let session_frames = plan_block_session_frames(&blocks, None, commit_flash);

    println!(
        "[DRY-RUN] Planned Block Session ({} blocks, {} frames total) for LED:",
        blocks.len(),
        session_frames.len()
    );
    for block in &blocks {
        if block.addr == 0x0038 {
            println!("  Block 0x0038: Bank 7 (LED brightness): 0x{:04x}", block.banks[7]);
        } else if block.addr == 0x0018 {
            println!("  Block 0x0018: Bank 4 (LED effect/speed): 0x{:04x}, Bank 0: 0x{:04x}", block.banks[4], block.banks[0]);
        }
    }
    if settle_ms > 0 {
        println!("  Timing delay: {} ms pause between frames.", settle_ms);
    }
    for (idx, f) in session_frames.iter().enumerate() {
        let hex = f.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
        println!("  Frame {:04}: [{}]", idx, hex);
    }
    if commit_flash {
        println!("  Commit tail: 5 frames (including mode 0x10 Flash Flush)");
    } else {
        println!("  Commit tail: 4 frames (Soft-Apply without Flash Flush)");
    }

    if gate.authorized(WriteGateLevel::Level2ConfigurationWrite) && !dry_run {
        let (target_path, mut dev) = open_device(device, WriteGateLevel::Level2ConfigurationWrite)?;
        authorize_device_write(&mut dev, &gate)?;
        print_gate_authorized(WriteGateLevel::Level2ConfigurationWrite, &target_path);

        println!(
            "Executing block session write transaction (settle_ms = {}, commit_flash = {})...",
            settle_ms, commit_flash
        );
        match execute_block_session(&mut dev, &blocks, false, commit_flash, settle_ms) {
            Ok(()) => {
                println!("LED configuration applied successfully via block session. Status: unverified (ACK verified).");
                if !no_ledger {
                    for (bank, addr, data) in recorded_writes {
                        record_write(&state_path, &target_path, bank, addr, data)?;
                    }
                    println!("Recorded writes to shadow ledger: {}", state_path.display());
                }
            }
            Err(e) => {
                eprintln!("Transaction error: {}", e);
                return Err(e);
            }
        }
    } else {
        if let Ok((_, dev)) = open_device(device, WriteGateLevel::Level0ReadOnly) {
            print_unverified_dry_run_warning_if_needed(&dev);
        }
        println!("\nNotice: Dry-run active by default. No data written to hardware.");
        println!("To apply configuration, pass Level 2 authorization flags:\n  --write --confirm");
    }

    Ok(())
}


#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use panteon_ms89::model::{ShadowEntry, ShadowLedger};

    #[test]
    pub fn test_append_entry_pure() {
        let mut ledger = ShadowLedger::new("test_dev");
        assert_eq!(ledger.entries.len(), 0);
        let entry = ShadowEntry::new(6, 24, 1792, "1712345678");
        append_entry(&mut ledger, entry.clone());
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(ledger.entries[0], entry);
    }

    #[test]
    pub fn test_resolve_state_path_custom() {
        let custom = PathBuf::from("/tmp/custom_ledger.toml");
        assert_eq!(resolve_state_path(Some(custom.clone())), custom);
    }

    #[test]
    pub fn test_save_and_load_ledger_atomic() {
        let temp_dir = std::env::temp_dir().join(format!("test_ledger_{}", std::process::id()));
        let ledger_path = temp_dir.join("subdir").join("ledger.toml");

        let mut ledger = ShadowLedger::new("/dev/hidraw1");
        ledger.append(ShadowEntry::new(6, 24, 1792, "1712345678"));

        save_ledger_atomic(&ledger_path, &ledger).expect("Save atomic should succeed");
        assert!(ledger_path.exists());

        let loaded = load_ledger(&ledger_path).expect("Load should succeed");
        assert_eq!(ledger, loaded);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    pub fn test_cli_dry_run_resolves_bank0_from_ledger() {
        let mut ledger = ShadowLedger::new("/dev/hidraw1");
        ledger.append(ShadowEntry::new(0, 0x0018, 0x071b, "1000")); // Slot 4

        let code = PollingRate::Hz500.to_register_code();
        let block18 = resolve_block18(&ledger, &[(registers::POLLING_RATE.0, code)]).unwrap();
        assert_eq!(block18.banks[0], 0x071b);
        assert_eq!(block18.banks[6], code);
    }

    #[test]
    pub fn test_cli_dry_run_with_active_slot_override() {
        let mut ledger = ShadowLedger::new("/dev/hidraw1");
        ledger.append(ShadowEntry::new(0, 0x0018, 0x0718, "1000")); // Slot 1 in ledger

        let code = PollingRate::Hz500.to_register_code();
        let raw_slot4 = DeviceState::slot_index_to_raw(4).unwrap();
        let block18 = resolve_block18(
            &ledger,
            &[
                (registers::POLLING_RATE.0, code),
                (registers::ACTIVE_DPI_SLOT.0, raw_slot4),
            ],
        )
        .unwrap();
        // Override wins over ledger
        assert_eq!(block18.banks[0], 0x071b);
        assert_eq!(block18.banks[6], code);
    }
}
