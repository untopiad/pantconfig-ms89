//! CLI commands for DPI slots configuration (M4.0).

use std::path::PathBuf;

use panteon_ms89::codec::{
    decode_dpi_slots, dpi_to_code, DEFAULT_DPI_SLOTS, DEFAULT_SECOND_COLORS, DEFAULT_SLOT_COLORS,
};
use panteon_ms89::device::WriteGateLevel;
use panteon_ms89::service::{
    execute_block_session, load_ledger, open_device,
    plan_block_session_frames, plan_color_session, plan_dpi_slots, plans_to_block_writes,
    record_write, resolve_state_path,
};
use panteon_ms89::PanteonError;

use super::args::{authorize_device_write, print_unverified_dry_run_warning_if_needed, GateArgs};
use super::render::print_gate_authorized;

/// Parses a 6-character hex color string (e.g. "#ff8800" or "ff8800") into RGB `(u8, u8, u8)`.
pub fn parse_hex_color(s: &str) -> Result<(u8, u8, u8), PanteonError> {
    let clean = s.trim().trim_start_matches('#');
    if clean.len() != 6 {
        return Err(PanteonError::ProtocolDetailed {
            context: "parse_hex_color",
            detail: format!("Hex color must be 6 hex characters (e.g. '#ff8800' or 'ff8800'), got '{}'", s),
        });
    }
    let r = u8::from_str_radix(&clean[0..2], 16).map_err(|e| PanteonError::ProtocolDetailed {
        context: "parse_hex_color",
        detail: format!("Invalid red hex '{}': {}", &clean[0..2], e),
    })?;
    let g = u8::from_str_radix(&clean[2..4], 16).map_err(|e| PanteonError::ProtocolDetailed {
        context: "parse_hex_color",
        detail: format!("Invalid green hex '{}': {}", &clean[2..4], e),
    })?;
    let b = u8::from_str_radix(&clean[4..6], 16).map_err(|e| PanteonError::ProtocolDetailed {
        context: "parse_hex_color",
        detail: format!("Invalid blue hex '{}': {}", &clean[4..6], e),
    })?;
    Ok((r, g, b))
}

/// Helper to reconstruct current 6 DPI slots from shadow ledger or default fallback.
pub fn resolve_current_dpi_slots(state_path: &std::path::Path) -> [u32; 6] {
    let ledger = load_ledger(state_path).unwrap_or_default();
    let find_latest = |bank: u8| -> Option<u8> {
        ledger
            .entries
            .iter()
            .rev()
            .find(|e| e.bank == bank && e.addr == 0x0010)
            .map(|e| (e.data & 0xff) as u8)
    };

    match (find_latest(3), find_latest(5), find_latest(6), find_latest(7)) {
        (Some(b3), Some(b5), Some(b6), Some(b7)) => {
            decode_dpi_slots(b3, b5, b6, b7).unwrap_or(DEFAULT_DPI_SLOTS)
        }
        _ => DEFAULT_DPI_SLOTS,
    }
}

/// Build transaction plans for setting all 6 DPI slots, optionally including full color session plans.
pub fn plan_set_dpi_slots(
    slots: &[u32; 6],
    colors: Option<&[(u8, u8, u8); 6]>,
    ledger: &panteon_ms89::model::ShadowLedger,
    brightness: Option<u8>,
) -> Result<Vec<panteon_ms89::device::WriteTransactionPlan>, PanteonError> {
    for &dpi in slots {
        dpi_to_code(dpi)?;
    }

    let mut plans = plan_dpi_slots(slots)?.to_vec();

    if let Some(c) = colors {
        let color_plans = plan_color_session(c, &DEFAULT_SECOND_COLORS, ledger, brightness)?;
        plans.extend(color_plans);
    }

    Ok(plans)
}

/// Configure all 6 DPI slots atomically, optionally with custom slot colors and block-oriented session.
#[allow(clippy::too_many_arguments)]
pub fn run_set_dpi_slots(
    slots: [u32; 6],
    colors: Option<[(u8, u8, u8); 6]>,
    brightness: Option<u8>,
    block_session: bool,
    device: Option<PathBuf>,
    dry_run: bool,
    settle_ms: u64,
    commit_flash: bool,
    state: Option<PathBuf>,
    no_ledger: bool,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let state_path = resolve_state_path(state.clone());
    let ledger = load_ledger(&state_path).unwrap_or_default();
    let plans = plan_set_dpi_slots(&slots, colors.as_ref(), &ledger, brightness)?;

    if block_session {
        let blocks = plans_to_block_writes(&plans);
        let session_frames = plan_block_session_frames(&blocks, None, commit_flash);
        println!(
            "[DRY-RUN] Planned Block Session ({} blocks, {} frames total) for 6 DPI SLOTS: {:?}",
            blocks.len(),
            session_frames.len(),
            slots
        );
        if let Some(ref c) = colors {
            println!("  Custom slot colors: {:?}", c);
            if let Some(b38) = blocks.iter().find(|b| b.addr == 0x0038) {
                println!("  Global LED brightness Bank 7 @ 0x0038: 0x{:04x}", b38.banks[7]);
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
                "Executing Block Session ({} blocks, {} frames, settle_ms = {}, commit_flash = {})...",
                blocks.len(),
                session_frames.len(),
                settle_ms,
                commit_flash
            );
            match execute_block_session(&mut dev, &blocks, false, commit_flash, settle_ms) {
                Ok(()) => {
                    println!("DPI configuration applied successfully via block session. Status: unverified (ACK verified).");
                    if !no_ledger {
                        let state_path = resolve_state_path(state);
                        for plan in &plans {
                            record_write(&state_path, &target_path, plan.bank, plan.addr, plan.data)?;
                        }
                        println!("Recorded {} register writes to shadow ledger: {}", plans.len(), state_path.display());
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
                "Dry-run complete. Pass --confirm --write to execute hardware write."
            );
        }
        return Ok(());
    }

    println!(
        "[DRY-RUN] Planned {} write transactions for 6 DPI SLOTS: {:?}",
        plans.len(),
        slots
    );
    if let Some(ref c) = colors {
        println!("  Custom slot colors: {:?}", c);
        if let Some(p38) = plans.iter().find(|p| p.addr == 0x0038 && p.bank == 7) {
            println!("  Global LED brightness Bank 7 @ 0x0038: 0x{:04x}", p38.data);
        }
    }
    if settle_ms > 0 {
        println!("  Timing delay: {} ms pause between frames.", settle_ms);
    }
    for (idx, plan) in plans.iter().enumerate() {
        println!("  Register {} (Bank {}, Addr 0x{:04x}, Data 0x{:04x}):", idx + 1, plan.bank, plan.addr, plan.data);
        for log_line in plan.log_frames() {
            println!("    {}", log_line);
        }
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

        if commit_flash {
            return execute_dpi_full_push(&mut dev, &target_path, &plans, state, None);
        }

        println!(
            "Executing {} DPI write transactions via block session (settle_ms = {}, commit_flash = {})...",
            plans.len(),
            settle_ms,
            commit_flash
        );
        let blocks = plans_to_block_writes(&plans);
        match execute_block_session(&mut dev, &blocks, false, commit_flash, settle_ms) {
            Ok(()) => {
                println!("DPI configuration applied successfully via block session. Status: unverified (ACK verified).");
                if !no_ledger {
                    let state_path = resolve_state_path(state);
                    for plan in &plans {
                        record_write(&state_path, &target_path, plan.bank, plan.addr, plan.data)?;
                    }
                    println!("Recorded {} register writes to shadow ledger: {}", plans.len(), state_path.display());
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
            "Dry-run complete. Pass --confirm --write to execute hardware write."
        );
    }
    Ok(())
}

/// Configure a single DPI slot (1..=6), preserving other slots from shadow ledger or factory defaults.
#[allow(clippy::too_many_arguments)]
pub fn run_set_dpi_slot(
    slot: u8,
    dpi: u32,
    color_hex: Option<String>,
    device: Option<PathBuf>,
    dry_run: bool,
    settle_ms: u64,
    commit_flash: bool,
    state: Option<PathBuf>,
    no_ledger: bool,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    if !(1..=6).contains(&slot) {
        return Err(PanteonError::ProtocolDetailed {
            context: "run_set_dpi_slot",
            detail: format!("Slot must be 1..=6, got {}", slot),
        });
    }
    dpi_to_code(dpi)?;

    let state_path = resolve_state_path(state.clone());
    let mut slots = resolve_current_dpi_slots(&state_path);
    let old_dpi = slots[(slot - 1) as usize];
    slots[(slot - 1) as usize] = dpi;

    println!(
        "Updating Slot {}: {} DPI -> {} DPI (current full profile: {:?})",
        slot, old_dpi, dpi, slots
    );

    let mut plans = plan_dpi_slots(&slots)?.to_vec();

    if let Some(ref hex) = color_hex {
        let rgb = parse_hex_color(hex)?;
        let mut colors = DEFAULT_SLOT_COLORS;
        colors[(slot - 1) as usize] = rgb;
        println!("  Updating Slot {} color: #{:02x}{:02x}{:02x}", slot, rgb.0, rgb.1, rgb.2);
        let ledger = load_ledger(&state_path).unwrap_or_default();
        let color_plans = plan_color_session(&colors, &DEFAULT_SECOND_COLORS, &ledger, None)?;
        plans.extend(color_plans);
    }

    println!("[DRY-RUN] Planned {} write transactions for slot update:", plans.len());
    for plan in &plans {
        for log_line in plan.log_frames() {
            println!("  {}", log_line);
        }
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

        if commit_flash {
            return execute_dpi_full_push(&mut dev, &target_path, &plans, state, Some(slot));
        }

        println!("Executing transactions via block session...");
        let blocks = plans_to_block_writes(&plans);
        match execute_block_session(&mut dev, &blocks, false, commit_flash, settle_ms) {
            Ok(()) => {
                println!("DPI slot updated successfully via block session. Status: unverified (ACK verified).");
                if !no_ledger {
                    for plan in &plans {
                        record_write(&state_path, &target_path, plan.bank, plan.addr, plan.data)?;
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
        println!(
            "Dry-run complete. Pass --confirm --write to execute hardware write."
        );
    }

    Ok(())
}

fn execute_dpi_full_push(
    dev: &mut panteon_ms89::device::Device,
    target_path: &std::path::Path,
    plans: &[panteon_ms89::device::WriteTransactionPlan],
    state: Option<PathBuf>,
    edited_slot: Option<u8>,
) -> Result<(), PanteonError> {
    let state_path = resolve_state_path(state);
    let mut ledger = load_ledger(&state_path).unwrap_or_default();
    let now = panteon_ms89::model::current_timestamp_secs();
    for plan in plans {
        ledger.append(panteon_ms89::model::ShadowEntry::new(plan.bank, plan.addr, plan.data, &now));
    }
    if let Some(slot) = edited_slot {
        if (1..=6).contains(&slot) {
            let active_slot_data = 0x0700 | (0x18 + (slot as u16) - 1);
            ledger.append(panteon_ms89::model::ShadowEntry::new(
                panteon_ms89::model::registers::ACTIVE_DPI_SLOT.0,
                panteon_ms89::model::registers::ACTIVE_DPI_SLOT.1,
                active_slot_data,
                &now,
            ));
        }
    }
    panteon_ms89::service::save_ledger_atomic(&state_path, &ledger)?;
    let snapshot = panteon_ms89::service::build_full_push_snapshot(&ledger)?;
    let mut mut_ledger = Some(&mut ledger);
    println!("Executing Full-Push 867-frame session with commit to SPI Flash...");
    panteon_ms89::service::restore_snapshot(dev, &snapshot, None, false, &mut mut_ledger)?;
    println!("DPI configuration applied and committed to Flash successfully!");
    let bus_dev = panteon_ms89::service::discover().ok().and_then(|devs| {
        devs.into_iter().find(|d| d.path == target_path).and_then(|d| match (d.usb_bus, d.usb_dev) {
            (Some(b), Some(d_num)) => Some((b, d_num)),
            _ => None,
        })
    });
    if let Some((b, d)) = bus_dev {
        let _ = panteon_ms89::service::soft_reset((b, d));
        println!("USB soft-reset triggered for /dev/bus/usb/{:03}/{:03}", b, d);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::args::parse_hex_color_arg;
    use crate::cli::definition::{Cli, Commands};
    use clap::Parser;

    #[test]
    fn test_set_dpi_slots_with_colors_counts() {
        let slots = [200, 400, 800, 1600, 3200, 6400];
        let mut ledger = panteon_ms89::model::ShadowLedger::default();
        ledger.append(panteon_ms89::model::ShadowEntry::new(7, 0x0038, 0x0700, "1000"));
        let plans_without = plan_set_dpi_slots(&slots, None, &ledger, None).expect("planning without colors should succeed");
        assert_eq!(plans_without.len(), 8);

        let colors: [(u8, u8, u8); 6] = [
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (255, 255, 0),
            (255, 0, 255),
            (0, 255, 255),
        ];
        let plans_with = plan_set_dpi_slots(&slots, Some(&colors), &ledger, None).expect("planning with colors should succeed");
        assert_eq!(plans_with.len(), 40);
        assert_eq!(plans_without.len() + 32, plans_with.len());

        // Verify CLI parsing without --colors flag yields None
        let cli_no_colors = Cli::try_parse_from([
            "panteon-ms89", "set-dpi-slots", "200", "400", "800", "1600", "3200", "6400",
        ]).expect("CLI parse without --colors should succeed");
        if let Commands::SetDpiSlots { colors: parsed_colors, .. } = cli_no_colors.command {
            assert!(parsed_colors.is_none());
        } else {
            panic!("Expected SetDpiSlots command");
        }

        // Verify CLI parsing with --colors flag yields 6 parsed RGB values and 40 plans
        let cli_with_colors = Cli::try_parse_from([
            "panteon-ms89", "set-dpi-slots", "200", "400", "800", "1600", "3200", "6400",
            "--colors", "#ff0000", "#00ff00", "#0000ff", "#ffff00", "#ff00ff", "#00ffff",
        ]).expect("CLI parse with --colors should succeed");
        if let Commands::SetDpiSlots { colors: parsed_colors, .. } = cli_with_colors.command {
            let c_vec = parsed_colors.expect("--colors should be Some");
            assert_eq!(c_vec.len(), 6);
            let c_arr: [(u8, u8, u8); 6] = [c_vec[0], c_vec[1], c_vec[2], c_vec[3], c_vec[4], c_vec[5]];
            let mut ledger = panteon_ms89::model::ShadowLedger::default();
            ledger.append(panteon_ms89::model::ShadowEntry::new(7, 0x0038, 0x0700, "1000"));
            let plans_from_cli = plan_set_dpi_slots(&slots, Some(&c_arr), &ledger, None).unwrap();
            assert_eq!(plans_from_cli.len(), 40);
        } else {
            panic!("Expected SetDpiSlots command");
        }
    }

    #[test]
    fn test_set_dpi_slots_colors_rgb_parse_rejects_bad_hex() {
        // Direct parse_hex_color_arg unit tests
        assert!(parse_hex_color_arg("invalid").is_err());
        assert!(parse_hex_color_arg("#12345").is_err()); // 5 characters
        assert!(parse_hex_color_arg("#1234567").is_err()); // 7 characters
        assert!(parse_hex_color_arg("#zz0000").is_err()); // non-hex
        assert!(parse_hex_color_arg("").is_err()); // empty

        // Valid formats accepted
        assert_eq!(parse_hex_color_arg("#ff8800").unwrap(), (255, 136, 0));
        assert_eq!(parse_hex_color_arg("00ff88").unwrap(), (0, 255, 136));

        // CLI try_parse_from rejection for bad hex
        let res_bad_hex = Cli::try_parse_from([
            "panteon-ms89", "set-dpi-slots", "200", "400", "800", "1600", "3200", "6400",
            "--colors", "#ff0000", "badhex", "#0000ff", "#ffff00", "#ff00ff", "#00ffff",
        ]);
        assert!(res_bad_hex.is_err());

        // CLI try_parse_from rejection for incorrect argument count (fewer than 6)
        let res_few = Cli::try_parse_from([
            "panteon-ms89", "set-dpi-slots", "200", "400", "800", "1600", "3200", "6400",
            "--colors", "#ff0000", "#00ff00",
        ]);
        assert!(res_few.is_err());
    }

    #[test]
    fn test_set_dpi_slots_block_session_flag_and_planning() {
        let cli_block = Cli::try_parse_from([
            "panteon-ms89", "set-dpi-slots", "200", "400", "800", "1600", "3200", "6400",
            "--block-session",
        ]).expect("CLI parse with --block-session should succeed");
        if let Commands::SetDpiSlots { block_session, .. } = cli_block.command {
            assert!(block_session);
        } else {
            panic!("Expected SetDpiSlots command");
        }

        let slots = [200, 800, 200, 800, 200, 800];
        let colors: [(u8, u8, u8); 6] = [
            (255, 0, 0), (0, 0, 255), (255, 0, 0),
            (0, 0, 255), (255, 0, 0), (0, 0, 255),
        ];
        let mut ledger = panteon_ms89::model::ShadowLedger::default();
        ledger.append(panteon_ms89::model::ShadowEntry::new(7, 0x0038, 0x0740, "1000")); // 50%
        let plans = plan_set_dpi_slots(&slots, Some(&colors), &ledger, None).unwrap();
        let blocks = plans_to_block_writes(&plans);
        assert_eq!(blocks.len(), 5);
        assert_eq!(blocks[0].addr, 0x0010);
        assert_eq!(blocks[1].addr, 0x0020);
        assert_eq!(blocks[2].addr, 0x0028);
        assert_eq!(blocks[3].addr, 0x0030);
        assert_eq!(blocks[4].addr, 0x0038);
        assert_eq!(blocks[4].banks[7], 0x0740);

        let frames_flush = plan_block_session_frames(&blocks, None, true);
        assert_eq!(frames_flush.len(), 58); // 3 preamble + 5 * 10 + 5 tail

        let frames_no_flush = plan_block_session_frames(&blocks, None, false);
        assert_eq!(frames_no_flush.len(), 57); // 3 preamble + 5 * 10 + 4 tail
    }
}
