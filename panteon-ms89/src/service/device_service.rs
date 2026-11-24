use std::path::PathBuf;

use crate::codec::DescriptorPage;
use crate::device::{
    discover_devices, select_device_path, BlockWrite, Device, DiscoveredDevice, Transport,
    WriteGateLevel, WriteTransactionPlan,
};
use crate::model::{registers, DeviceState, PackedMemLayout, PackedMemMode, PollingRate};
use crate::{PanteonError, FEATURE_REPORT_LEN};

/// Discover all connected PANTEON MS89 devices
pub fn discover() -> Result<Vec<DiscoveredDevice>, PanteonError> {
    discover_devices()
}

/// Open device with specified gate level, choosing target path automatically if None
pub fn open_device(
    device_path: Option<PathBuf>,
    gate_level: WriteGateLevel,
) -> Result<(PathBuf, Device), PanteonError> {
    let path = select_device_path(device_path)?;
    let dev = Device::open_with_gate_level(&path, gate_level)?;
    Ok((path, dev))
}

/// Read device state and handshake
pub fn read_handshake(device: &mut Device) -> Result<DeviceState, PanteonError> {
    device.read_state()
}

/// Read and decode 32-byte descriptor page
pub fn read_descriptor(device: &mut Device) -> Result<DescriptorPage, PanteonError> {
    device.read_descriptor_page()
}

/// Build WriteTransactionPlan for setting polling rate
pub fn plan_polling_rate(rate_hz: u16) -> Result<WriteTransactionPlan, PanteonError> {
    let rate = PollingRate::from_hz(rate_hz).ok_or(PanteonError::Protocol(
        "Invalid polling rate (must be 125, 250, 500, or 1000)",
    ))?;
    Ok(WriteTransactionPlan::new(
        registers::POLLING_RATE.0,
        registers::POLLING_RATE.1,
        rate.to_register_code(),
    ))
}

/// Build WriteTransactionPlan for active DPI slot (1..=6)
pub fn plan_dpi_slot(slot: u8) -> Result<WriteTransactionPlan, PanteonError> {
    let raw_val = DeviceState::slot_index_to_raw(slot)?;
    Ok(WriteTransactionPlan::new(
        registers::ACTIVE_DPI_SLOT.0,
        registers::ACTIVE_DPI_SLOT.1,
        raw_val,
    ))
}

/// Build WriteTransactionPlan for LED effect mode and speed
/// - `effect_nibble`: 4-bit mode (e.g. 0x8 for DPI-Breath, 0x9 for RGB-Breath, 0xd for Neon, 0xf for Default)
/// - `speed_nibble`: 4-bit speed (0..=7, 7 is LED Off in 0x9_ mode)
pub fn plan_led_effect_speed(effect_nibble: u8, speed_nibble: u8) -> Result<WriteTransactionPlan, PanteonError> {
    if effect_nibble > 0x0f || speed_nibble > 0x0f {
        return Err(PanteonError::Protocol("Effect and speed must be 4-bit nibbles (0..=15)"));
    }
    let byte_val = (effect_nibble << 4) | (speed_nibble & 0x0f);
    let data_val = 0x0700 | (byte_val as u16);
    Ok(WriteTransactionPlan::new(
        registers::LED_EFFECT_SPEED.0,
        registers::LED_EFFECT_SPEED.1,
        data_val,
    ))
}

/// Build WriteTransactionPlan for LED color palette bitmask
/// - `color_mask`: 0xff for all colors, or 0x80 | \sum (1 << c) for subset of 7 colors
pub fn plan_led_palette(color_mask: u8) -> Result<WriteTransactionPlan, PanteonError> {
    let data_val = 0x0700 | (color_mask as u16);
    Ok(WriteTransactionPlan::new(
        registers::LED_COLOR_PALETTE.0,
        registers::LED_COLOR_PALETTE.1,
        data_val,
    ))
}

/// Build WriteTransactionPlans for setting fire button delay (8..=300 ms).
/// Returns `(low_byte_plan, high_byte_plan)` for Bank 2 and Bank 3 at Addr 0x0018.
pub fn plan_fire_delay(delay_ms: u16) -> Result<(WriteTransactionPlan, WriteTransactionPlan), PanteonError> {
    if !(8..=300).contains(&delay_ms) {
        return Err(PanteonError::Protocol("Fire delay must be between 8 and 300 ms"));
    }
    let low_plan = WriteTransactionPlan::new(
        registers::FIRE_DELAY_LOW.0,
        registers::FIRE_DELAY_LOW.1,
        0x0700 | (delay_ms & 0xff),
    );
    let high_plan = WriteTransactionPlan::new(
        registers::FIRE_DELAY_HIGH.0,
        registers::FIRE_DELAY_HIGH.1,
        0x0700 | ((delay_ms >> 8) & 0xff),
    );
    Ok((low_plan, high_plan))
}

/// Build WriteTransactionPlans for all 6 DPI slots (Banks 0..7 @ 0x0010)
pub fn plan_dpi_slots(slots: &[u32; 6]) -> Result<[WriteTransactionPlan; 8], PanteonError> {
    crate::codec::encode_dpi_slots(slots)
}

/// Build WriteTransactionPlans for full color session across blocks 0x0020, 0x0028, 0x0030, 0x0038.
/// Bank 7 of 0x0038 is resolved strictly via: user override -> ShadowLedger -> PanteonError::Protocol (R8).
pub fn plan_color_session(
    main: &[(u8, u8, u8); 6],
    second: &[(u8, u8, u8); 6],
    ledger: &crate::model::ShadowLedger,
    brightness_override: Option<u8>,
) -> Result<Vec<WriteTransactionPlan>, PanteonError> {
    let block38 = crate::service::resolve_block38(ledger, brightness_override)?;
    let mut plans = Vec::with_capacity(32);

    let mut channels = [0u8; 18];
    for slot in 0..6 {
        let s = crate::model::DpiSlot {
            level_index: (slot + 1) as u8,
            dpi: 0,
            enabled: true,
            main: [main[slot].0, main[slot].1, main[slot].2],
            second: [second[slot].0, second[slot].1, second[slot].2],
        };
        let pwm = s.to_pwm_bytes();
        channels[slot * 3] = pwm[0];
        channels[slot * 3 + 1] = pwm[1];
        channels[slot * 3 + 2] = pwm[2];
    }

    // Addr 0x0020, Banks 0..7
    for bank in 0..8u8 {
        plans.push(WriteTransactionPlan::new(
            bank,
            0x0020,
            0x0700 | (channels[bank as usize] as u16),
        ));
    }

    // Addr 0x0028, Banks 0..7
    for bank in 0..8u8 {
        plans.push(WriteTransactionPlan::new(
            bank,
            0x0028,
            0x0700 | (channels[8 + bank as usize] as u16),
        ));
    }

    // Addr 0x0030, Banks 0..7
    let b0_30 = channels[16];
    let b1_30 = channels[17];
    let bytes_30: [u8; 8] = [b0_30, b1_30, 0xff, 0x7f, 0x00, 0x00, 0x00, 0x00];
    for bank in 0..8u8 {
        plans.push(WriteTransactionPlan::new(
            bank,
            0x0030,
            0x0700 | (bytes_30[bank as usize] as u16),
        ));
    }

    // Addr 0x0038, Banks 0..7
    for bank in 0..8u8 {
        plans.push(WriteTransactionPlan::new(
            bank,
            0x0038,
            block38.banks[bank as usize],
        ));
    }

    Ok(plans)
}

/// Deprecated: use `plan_color_session(main, second, ledger, brightness)` instead (ADR-003, R8).
#[deprecated(since = "0.2.0", note = "Use plan_color_session(main, second, ledger, brightness) instead")]
pub fn plan_slot_colors(colors: &[(u8, u8, u8); 6]) -> Result<Vec<WriteTransactionPlan>, PanteonError> {
    plan_color_session(colors, &crate::codec::DEFAULT_SECOND_COLORS, &crate::model::ShadowLedger::default(), Some(8))
}

/// Execute transaction plan with settle delay
pub fn execute_plan(
    device: &mut Device,
    plan: &WriteTransactionPlan,
    dry_run: bool,
    settle_ms: u64,
) -> Result<(), PanteonError> {
    device.execute_transaction_with_settle(plan, dry_run, settle_ms)
}

/// Execute transaction plan with Soft-Apply commit tail.
///
/// If `dry_run` is true, the plan is not written and tail frames are skipped.
/// When `flash_flush` is true, issues mode 0x10 to commit state to SPI Flash (user-level Apply only).
pub fn execute_with_commit<T: Transport + ?Sized>(
    transport: &mut T,
    plan: &WriteTransactionPlan,
    dry_run: bool,
    flash_flush: bool,
) -> Result<(), PanteonError> {
    execute_with_commit_settle(transport, plan, dry_run, flash_flush, 0)
}

/// Returns true if the register address belongs to a multi-bank block that requires atomic block sessions (ADR-003 §7, §8).
///
/// Multi-bank blocks:
/// - 0x0010: DPI slots configuration (8 banks)
/// - 0x0018: Device configuration (8 banks: slot, fire delay, LED, polling)
/// - 0x0020..=0x0038: Color session blocks (0x0020, 0x0028, 0x0030, 0x0038, 8 banks each)
pub fn is_multi_bank_block(addr: u16) -> bool {
    addr == 0x0010 || addr == 0x0018 || (0x0020..=0x0038).contains(&addr)
}

/// Execute a sequence of transaction plans followed by the Soft-Apply commit tail.
///
/// Multi-bank blocks (0x0010, 0x0018, 0x0020..=0x0038) are strictly forbidden
/// here because per-plan commits cause the controller to latch/finalize the entire
/// block prematurely, corrupting neighboring register values (ADR-003 §7, §8).
pub fn execute_plans_with_commit<T: Transport + ?Sized>(
    transport: &mut T,
    plans: &[WriteTransactionPlan],
    dry_run: bool,
    flash_flush: bool,
    settle_ms: u64,
) -> Result<(), PanteonError> {
    for plan in plans {
        if is_multi_bank_block(plan.addr) {
            return Err(PanteonError::Protocol(
                "multi-bank block requires block session — see ADR-003 §7",
            ));
        }
    }

    if dry_run {
        return Ok(());
    }

    for plan in plans {
        transport.execute_transaction_with_settle(plan, false, settle_ms)?;
    }

    let tail_frames = crate::codec::encode_soft_apply_tail_frames(flash_flush);
    for frame in &tail_frames {
        transport.write_feature(frame)?;
        if settle_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(settle_ms));
        }
    }

    Ok(())
}

/// Execute transaction plan with Soft-Apply commit tail and inter-frame timing delay.
pub fn execute_with_commit_settle<T: Transport + ?Sized>(
    transport: &mut T,
    plan: &WriteTransactionPlan,
    dry_run: bool,
    flash_flush: bool,
    settle_ms: u64,
) -> Result<(), PanteonError> {
    execute_plans_with_commit(transport, std::slice::from_ref(plan), dry_run, flash_flush, settle_ms)
}

/// Execute Fire delay write transactions (Bank 2 and Bank 3 @ 0x0018) via block session.
pub fn execute_fire_delay<T: Transport + ?Sized>(
    transport: &mut T,
    delay_ms: u16,
    dry_run: bool,
    flash_flush: bool,
    settle_ms: u64,
) -> Result<(), PanteonError> {
    execute_fire_delay_with_ledger(
        transport,
        delay_ms,
        &crate::model::ShadowLedger::default(),
        dry_run,
        flash_flush,
        settle_ms,
    )
}

/// Execute Fire delay write transactions with explicit shadow ledger via block session.
pub fn execute_fire_delay_with_ledger<T: Transport + ?Sized>(
    transport: &mut T,
    delay_ms: u16,
    ledger: &crate::model::ShadowLedger,
    dry_run: bool,
    flash_flush: bool,
    settle_ms: u64,
) -> Result<(), PanteonError> {
    let (p_low, p_high) = plan_fire_delay(delay_ms)?;
    let block = crate::service::resolve_block18(
        ledger,
        &[
            (registers::FIRE_DELAY_LOW.0, p_low.data),
            (registers::FIRE_DELAY_HIGH.0, p_high.data),
        ],
    )?;
    execute_block_session(transport, &[block], dry_run, flash_flush, settle_ms)
}

/// Execute Active DPI slot configuration via block session and record write to shadow ledger.
pub fn execute_active_dpi_slot<T: Transport + ?Sized>(
    transport: &mut T,
    slot: u8,
    ledger: &mut crate::model::ShadowLedger,
    dry_run: bool,
    flash_flush: bool,
    settle_ms: u64,
) -> Result<(), PanteonError> {
    let raw_val = crate::model::DeviceState::slot_index_to_raw(slot)?;
    let block = crate::service::resolve_block18(ledger, &[(registers::ACTIVE_DPI_SLOT.0, raw_val)])?;
    execute_block_session(transport, &[block], dry_run, flash_flush, settle_ms)?;
    if !dry_run {
        ledger.append(crate::model::ShadowEntry::new(
            registers::ACTIVE_DPI_SLOT.0,
            registers::ACTIVE_DPI_SLOT.1,
            raw_val,
            crate::model::current_timestamp_secs(),
        ));
    }
    Ok(())
}

/// Execute Polling rate configuration via block session.
pub fn execute_polling_rate<T: Transport + ?Sized>(
    transport: &mut T,
    rate_hz: u16,
    ledger: &crate::model::ShadowLedger,
    dry_run: bool,
    flash_flush: bool,
    settle_ms: u64,
) -> Result<(), PanteonError> {
    let plan = plan_polling_rate(rate_hz)?;
    let block = crate::service::resolve_block18(ledger, &[(registers::POLLING_RATE.0, plan.data)])?;
    execute_block_session(transport, &[block], dry_run, flash_flush, settle_ms)
}

/// Execute LED effect mode and speed via block session.
pub fn execute_led_effect_speed<T: Transport + ?Sized>(
    transport: &mut T,
    effect_nibble: u8,
    speed_nibble: u8,
    ledger: &crate::model::ShadowLedger,
    dry_run: bool,
    flash_flush: bool,
    settle_ms: u64,
) -> Result<(), PanteonError> {
    let plan = plan_led_effect_speed(effect_nibble, speed_nibble)?;
    let block = crate::service::resolve_block18(ledger, &[(registers::LED_EFFECT_SPEED.0, plan.data)])?;
    execute_block_session(transport, &[block], dry_run, flash_flush, settle_ms)
}

/// Execute LED color palette bitmask via block session.
pub fn execute_led_palette<T: Transport + ?Sized>(
    transport: &mut T,
    color_mask: u8,
    ledger: &crate::model::ShadowLedger,
    dry_run: bool,
    flash_flush: bool,
    settle_ms: u64,
) -> Result<(), PanteonError> {
    let plan = plan_led_palette(color_mask)?;
    let block = crate::service::resolve_block18(ledger, &[(registers::LED_COLOR_PALETTE.0, plan.data)])?;
    execute_block_session(transport, &[block], dry_run, flash_flush, settle_ms)
}

/// Execute LED brightness configuration via block session on block 0x0038 and record to shadow ledger (R8).
pub fn execute_led_brightness<T: Transport + ?Sized>(
    transport: &mut T,
    brightness: u8,
    ledger: &mut crate::model::ShadowLedger,
    dry_run: bool,
    flash_flush: bool,
    settle_ms: u64,
) -> Result<(), PanteonError> {
    let block = crate::service::resolve_block38(ledger, Some(brightness))?;
    let b7 = block.banks[7];
    execute_block_session(transport, &[block], dry_run, flash_flush, settle_ms)?;
    if !dry_run {
        ledger.append(crate::model::ShadowEntry::new(
            registers::LED_BRIGHTNESS.0,
            registers::LED_BRIGHTNESS.1,
            b7,
            crate::model::current_timestamp_secs(),
        ));
    }
    Ok(())
}

/// Convert a list of write transaction plans into 8-bank BlockWrites grouped by address.
pub fn plans_to_block_writes(plans: &[WriteTransactionPlan]) -> Vec<BlockWrite> {
    let mut blocks: Vec<BlockWrite> = Vec::new();
    let mut current_addr: Option<u16> = None;
    let mut current_banks = [0u16; 8];

    for plan in plans {
        if Some(plan.addr) != current_addr {
            if let Some(addr) = current_addr {
                blocks.push(BlockWrite::new(addr, current_banks));
                current_banks = [0u16; 8];
            }
            current_addr = Some(plan.addr);
        }
        if (plan.bank as usize) < 8 {
            current_banks[plan.bank as usize] = plan.data;
        }
    }
    if let Some(addr) = current_addr {
        blocks.push(BlockWrite::new(addr, current_banks));
    }
    blocks
}

/// Build the complete, ordered sequence of wire frames for a block session.
pub fn plan_block_session_frames(
    blocks: &[BlockWrite],
    profile: Option<u8>,
    flash_flush: bool,
) -> Vec<[u8; FEATURE_REPORT_LEN]> {
    let mut frames = Vec::new();

    // 1. Preamble (3 frames: P1 Data Write, P2 Commit Bank, P3 Final Commit)
    let p_idx = if let Some(p) = profile {
        p & 0x03
    } else if let Some(b70) = blocks.iter().find(|b| b.addr == 0x0070) {
        (b70.banks[6] & 0x03) as u8
    } else {
        1 // Default to Profile 1 (0x23) matching live hardware trace
    };
    let unlock_val = 0x0003 | ((p_idx as u16) << 5);

    frames.push(PackedMemLayout::new(PackedMemMode::Write as u8, 0x00, 0x0001, unlock_val).pack());
    frames.push(PackedMemLayout::new(PackedMemMode::CommitBank as u8, 0x00, 0x0001, 0x0000).pack());
    frames.push(PackedMemLayout::new(PackedMemMode::FinalCommit as u8, 0x00, 0x0001, 0x0000).pack());

    // 2. Blocks
    for block in blocks {
        for bank in 0..8u8 {
            frames.push(
                PackedMemLayout::new(
                    PackedMemMode::Write as u8,
                    bank,
                    block.addr,
                    block.banks[bank as usize],
                )
                .pack(),
            );
        }
        // Commit bank (mode 0x09): data is 0x0700 (byte 6 = 0x00, byte 7 = 0x07)
        frames.push(
            PackedMemLayout::new(PackedMemMode::CommitBank as u8, 0x00, block.addr, 0x0700).pack(),
        );
        // Final commit (mode 0x00): data is 0x0000
        frames.push(
            PackedMemLayout::new(PackedMemMode::FinalCommit as u8, 0x00, block.addr, 0x0000).pack(),
        );
    }

    // 3. Tail: Soft-apply commit tail
    let tail = crate::codec::encode_soft_apply_tail_frames(flash_flush);
    if blocks.iter().any(|b| b.addr == 0x007f) {
        for f in tail {
            if f[4] == 0x00 && f[5] == 0x00 {
                frames.push(f);
            }
        }
    } else {
        frames.extend(tail);
    }

    frames
}

/// Execute a block-oriented configuration session according to live golden trace structure.
pub fn execute_block_session<T: Transport + ?Sized>(
    transport: &mut T,
    blocks: &[BlockWrite],
    dry_run: bool,
    flash_flush: bool,
    settle_ms: u64,
) -> Result<(), PanteonError> {
    execute_block_session_with_profile(transport, blocks, None, dry_run, flash_flush, settle_ms)
}

/// Execute a block-oriented configuration session with explicit profile index.
pub fn execute_block_session_with_profile<T: Transport + ?Sized>(
    transport: &mut T,
    blocks: &[BlockWrite],
    profile: Option<u8>,
    dry_run: bool,
    flash_flush: bool,
    settle_ms: u64,
) -> Result<(), PanteonError> {
    if dry_run {
        return Ok(());
    }
    if transport.write_gate_level() < WriteGateLevel::Level2ConfigurationWrite {
        return Err(PanteonError::WriteDisabled);
    }
    if !transport.device_identity().protocol_verified {
        return Err(PanteonError::ProtocolDetailed {
            context: "execute_block_session",
            detail: format!(
                "protocol unverified for PID 0x{:04x} — run verify-protocol first",
                transport.device_identity().pid
            ),
        });
    }

    let frames = plan_block_session_frames(blocks, profile, flash_flush);
    for frame in &frames {
        transport.write_feature(frame)?;
        if settle_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(settle_ms));
        }
    }

    Ok(())
}

/// Triggers USB soft-reset via ioctl USBDEVFS_RESET (0x5514) on /dev/bus/usb/BBB/DDD.
///
/// Causes full USB re-enumeration. NOTE (live descriptor observations): bInterval in the endpoint
/// descriptor is a static firmware constant and never changes with polling rate; the
/// historical rationale "applies bInterval changes" is refuted. Soft-reset is kept as
/// the default post-write step for set-polling until live-apply is proven
/// (R10.1: `set-polling --no-soft-reset` under usbmon capture), and may be skipped
/// explicitly via `--no-soft-reset`.
pub fn soft_reset(bus_id: (u16, u16)) -> Result<(), PanteonError> {
    soft_reset_usb(bus_id.0, bus_id.1)
}

/// Triggers USB soft-reset for given bus and dev numbers.
pub fn soft_reset_usb(bus: u16, dev: u16) -> Result<(), PanteonError> {
    let usb_path = PathBuf::from(format!("/dev/bus/usb/{:03}/{:03}", bus, dev));
    #[cfg(target_os = "linux")]
    {
        use std::fs::OpenOptions;
        use std::os::unix::io::AsRawFd;

        const USBDEVFS_RESET: std::os::raw::c_ulong = 0x5514;
        extern "C" {
            fn ioctl(fd: std::os::raw::c_int, req: std::os::raw::c_ulong, ...) -> std::os::raw::c_int;
        }

        let file = OpenOptions::new()
            .write(true)
            .open(&usb_path)
            .map_err(|e| {
                if e.raw_os_error() == Some(13) || e.kind() == std::io::ErrorKind::PermissionDenied {
                    PanteonError::Io(format!(
                        "EACCES opening {}: нужен udev-доступ к usb-устройству (проверьте 99-panteon-ms89.rules)",
                        usb_path.display()
                    ))
                } else {
                    PanteonError::Io(format!("Failed to open {}: {}", usb_path.display(), e))
                }
            })?;

        let fd = file.as_raw_fd();
        let ret = unsafe { ioctl(fd, USBDEVFS_RESET, 0) };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(13) || err.kind() == std::io::ErrorKind::PermissionDenied {
                return Err(PanteonError::Io(format!(
                    "EACCES resetting {}: нужен udev-доступ к usb-устройству",
                    usb_path.display()
                )));
            }
            return Err(PanteonError::Io(format!(
                "USBDEVFS_RESET failed on {}: {}",
                usb_path.display(),
                err
            )));
        }
        drop(file);
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = usb_path;
        Err(PanteonError::Io("USB soft-reset is only supported on Linux".to_string()))
    }
}


