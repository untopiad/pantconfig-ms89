use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use panteon_ms89::device::{select_device_path, Device, Transport, WriteGateLevel};
use panteon_ms89::model::{current_timestamp_secs, ShadowEntry, ShadowLedger, StateSnapshot};
use panteon_ms89::service::{
    build_restore_plan, import_capture, load_ledger, load_snapshot_from_file, resolve_state_path,
    restore_snapshot, save_ledger_atomic, save_snapshot_to_file,
};
use panteon_ms89::{PanteonError, FEATURE_REPORT_LEN, REPORT_ID};

use super::args::{authorize_device_write, print_unverified_dry_run_warning_if_needed, GateArgs};
use super::render::print_gate_authorized;

pub fn run_capture_import(
    pcapng: PathBuf,
    label: String,
    pid: u16,
    out: Option<PathBuf>,
) -> Result<(), PanteonError> {
    let bytes = std::fs::read(&pcapng).map_err(|e| {
        PanteonError::Io(format!("Failed to read pcapng file '{}': {}", pcapng.display(), e))
    })?;

    let snapshot = import_capture(&bytes, pid, &label)?;
    let out_path = out.unwrap_or_else(|| PathBuf::from("snapshot.toml"));

    save_snapshot_to_file(&out_path, &snapshot)?;

    print_import_summary(&pcapng, &out_path, &snapshot);
    Ok(())
}

pub fn print_import_summary(pcapng: &Path, out_path: &Path, snapshot: &StateSnapshot) {
    println!("Imported capture from: {}", pcapng.display());
    println!("Label:                 {}", snapshot.label);
    println!("Source PID:            0x{:04x}", snapshot.source_pid);
    println!("Captured at:           {}", snapshot.captured_at);
    println!("Total frames:          {}", snapshot.frames.len());
    println!();
    println!("Command distribution:");

    let mut cmd_counts: BTreeMap<u8, usize> = BTreeMap::new();
    let mut mode_counts: BTreeMap<u8, usize> = BTreeMap::new();

    for frame in &snapshot.frames {
        let cmd = frame[1];
        *cmd_counts.entry(cmd).or_default() += 1;
        if cmd == 0x18 {
            let mode = frame[2];
            *mode_counts.entry(mode).or_default() += 1;
        }
    }

    for (cmd, count) in &cmd_counts {
        println!("  0x{:02x}: {}", cmd, count);
        if *cmd == 0x18 {
            for (mode, m_count) in &mode_counts {
                println!("    mode 0x{:02x}: {}", mode, m_count);
            }
        }
    }

    println!();
    println!("Snapshot written to:   {}", out_path.display());
}

pub fn run_restore_state(
    snapshot_path: PathBuf,
    device_path: Option<PathBuf>,
    step: bool,
    settle_override: Option<u64>,
    force_pid_mismatch: bool,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let snapshot = load_snapshot_from_file(&snapshot_path)?;
    let dev_path = select_device_path(device_path)?;

    if gate.level() < WriteGateLevel::Level2ConfigurationWrite {
        // Dry-run mode
        let dev = Device::open_with_gate_level(&dev_path, WriteGateLevel::Level0ReadOnly)?;
        let device_pid = dev.identity().pid;
        let pid_mismatch = snapshot.source_pid != device_pid;

        if pid_mismatch {
            eprintln!(
                "WARNING: Snapshot source PID (0x{:04x}) differs from target device PID (0x{:04x})",
                snapshot.source_pid, device_pid
            );
        }

        println!(
            "Dry-run mode: State restore plan ({} frames, label '{}', source PID 0x{:04x}):",
            snapshot.frames.len(),
            snapshot.label,
            snapshot.source_pid
        );

        let plan = build_restore_plan(&snapshot, settle_override);
        for item in plan {
            let hex_str = item.frame.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
            println!("  [{:04}] data=[{}] delay={}ms", item.index, hex_str, item.delay_ms);
        }

        println!();
        if pid_mismatch {
            println!("Notice: To execute restore across different PIDs, specify --force-pid-mismatch.");
        }
        print_unverified_dry_run_warning_if_needed(&dev);
        println!("Dry-run complete. To apply changes to device, authorize Level 2 write: --write --confirm");
        return Ok(());
    }

    // Live mode: Level 2 write required
    if !gate.authorized(WriteGateLevel::Level2ConfigurationWrite) {
        return Err(PanteonError::WriteDisabled);
    }

    let mut dev = Device::open_with_gate_level(&dev_path, gate.level())?;
    authorize_device_write(&mut dev, &gate)?;

    if snapshot.source_pid != dev.identity().pid && !force_pid_mismatch {
        return Err(PanteonError::ProtocolDetailed {
            context: "restore_state",
            detail: format!(
                "PID mismatch: snapshot source PID is 0x{:04x}, device PID is 0x{:04x}. Pass --force-pid-mismatch to override.",
                snapshot.source_pid, dev.identity().pid
            ),
        });
    }

    let state_path = resolve_state_path(None);
    let mut ledger = load_ledger(&state_path).unwrap_or_else(|_| ShadowLedger::new(dev_path.to_string_lossy()));

    print_gate_authorized(WriteGateLevel::Level2ConfigurationWrite, &dev_path);
    println!(
        "Restoring state from '{}' to {} ({} frames)...",
        snapshot_path.display(),
        dev_path.display(),
        snapshot.frames.len()
    );

    if step {
        // Step-by-step interactive mode
        for (i, (frame, default_delay)) in snapshot.frames.iter().zip(snapshot.delays_ms.iter()).enumerate() {
            let delay = settle_override.unwrap_or(*default_delay as u64);
            let hex_str = frame.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");

            use std::io::Write;
            print!(
                "Frame [{}/{}] data=[{}] delay={}ms. Send? [y/N]: ",
                i + 1,
                snapshot.frames.len(),
                hex_str,
                delay
            );
            std::io::stdout().flush().map_err(|e| PanteonError::Io(e.to_string()))?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).map_err(|e| PanteonError::Io(e.to_string()))?;
            let trimmed = input.trim();
            if trimmed != "y" && trimmed != "Y" && trimmed != "yes" {
                println!("Aborted by user at frame [{}].", i);
                return Ok(());
            }

            dev.raw_write_feature(frame).map_err(|e| PanteonError::ProtocolDetailed {
                context: "restore_state",
                detail: format!("frame [{}] {} write failed: {}", i, hex_str, e),
            })?;

            let mut ack = [0u8; FEATURE_REPORT_LEN];
            ack[0] = REPORT_ID;
            dev.read_feature(&mut ack).map_err(|e| PanteonError::ProtocolDetailed {
                context: "restore_state",
                detail: format!("frame [{}] {} ACK read failed: {}", i, hex_str, e),
            })?;
            if ack[0] != REPORT_ID {
                return Err(PanteonError::ProtocolDetailed {
                    context: "restore_state",
                    detail: format!(
                        "frame [{}] {} ACK mismatch: expected report_id 0x{:02x}, got 0x{:02x}",
                        i, hex_str, REPORT_ID, ack[0]
                    ),
                });
            }

            if frame[1] == 0x18 && frame[2] == 0x03 {
                let bank = frame[3];
                let addr = u16::from_le_bytes([frame[4], frame[5]]);
                let data = u16::from_le_bytes([frame[6], frame[7]]);
                ledger.append(ShadowEntry::new(bank, addr, data, current_timestamp_secs()));
            }

            if delay > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay));
            }
        }
    } else {
        let mut opt_ledger = Some(&mut ledger);
        restore_snapshot(&mut dev, &snapshot, settle_override, force_pid_mismatch, &mut opt_ledger)?;
    }

    save_ledger_atomic(&state_path, &ledger)?;
    println!("State restore completed successfully. Shadow ledger updated at {}", state_path.display());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn run_push_state(
    full: bool,
    device: Option<PathBuf>,
    state: Option<PathBuf>,
    settle_override: Option<u64>,
    force_pid_mismatch: bool,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let state_path = resolve_state_path(state);
    let ledger = load_ledger(&state_path).unwrap_or_default();
    println!(
        "Building full-push state from {} ({} recorded ledger entries)...",
        state_path.display(),
        ledger.entries.len()
    );

    let snapshot = crate::service::build_full_push_snapshot(&ledger)?;
    println!(
        "Synthesized full-push snapshot '{}': {} frames, source PID 0x{:04x}",
        snapshot.label,
        snapshot.frames.len(),
        snapshot.source_pid
    );

    if !gate.authorized(WriteGateLevel::Level2ConfigurationWrite) {
        println!("[DRY-RUN] Full-push 867-frame sequence planned (full: {}).", full);
        if let Ok(target) = select_device_path(device) {
            if let Ok(dev) = Device::open_with_gate_level(&target, WriteGateLevel::Level0ReadOnly) {
                print_unverified_dry_run_warning_if_needed(&dev);
            }
        }
        println!("Pass --confirm --write to execute hardware write.");
        return Ok(());
    }

    let dev_path = select_device_path(device)?;
    let mut dev = Device::open_with_gate_level(&dev_path, WriteGateLevel::Level2ConfigurationWrite)?;
    authorize_device_write(&mut dev, &gate)?;

    if snapshot.source_pid != dev.identity().pid && !force_pid_mismatch {
        return Err(PanteonError::ProtocolDetailed {
            context: "push_state",
            detail: format!(
                "PID mismatch: snapshot source PID is 0x{:04x}, device PID is 0x{:04x}. Pass --force-pid-mismatch to override.",
                snapshot.source_pid, dev.identity().pid
            ),
        });
    }

    print_gate_authorized(WriteGateLevel::Level2ConfigurationWrite, &dev_path);
    println!("Pushing full state to device (867 frames, ~30s)...");
    let mut ledger_val = load_ledger(&state_path).unwrap_or_default();
    let mut mut_ledger = Some(&mut ledger_val);
    restore_snapshot(&mut dev, &snapshot, settle_override, force_pid_mismatch, &mut mut_ledger)?;
    println!("Full state push completed successfully! Triggering USB soft-reset...");

    let bus_dev = crate::service::discover().ok().and_then(|devs| {
        devs.into_iter().find(|d| d.path == dev_path).and_then(|d| match (d.usb_bus, d.usb_dev) {
            (Some(b), Some(dev)) => Some((b, dev)),
            _ => None,
        })
    });
    if let Some((b, d)) = bus_dev {
        let _ = crate::service::soft_reset((b, d));
        println!("USB soft-reset triggered for /dev/bus/usb/{:03}/{:03}", b, d);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_import_cli_summary() {
        let test_frames = vec![
            [0x07, 0x18, 0x03, 0x00, 0x01, 0x00, 0x03, 0x00],
            [0x07, 0x18, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00],
            [0x07, 0x18, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00],
        ];
        let snapshot = StateSnapshot::new(0x1440, "1712345678", "summary_test", test_frames, vec![0, 10, 10]).unwrap();
        print_import_summary(Path::new("dummy.pcapng"), Path::new("dummy.toml"), &snapshot);
    }

    #[test]
    fn test_restore_state_dry_run() {
        let temp_dir = std::env::temp_dir().join(format!("test_snap_{}", std::process::id()));
        let snap_file = temp_dir.join("snap.toml");
        let test_frames = vec![
            [0x07, 0x18, 0x03, 0x00, 0x01, 0x00, 0x03, 0x00],
            [0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ];
        let snapshot = StateSnapshot::new(0x1440, "1712345678", "dry_run_test", test_frames, vec![0, 5]).unwrap();
        save_snapshot_to_file(&snap_file, &snapshot).unwrap();

        let gate = GateArgs::default();
        let dev = Some(PathBuf::from("/dev/nonexistent_ms89_test_dev"));
        let res = run_restore_state(snap_file, dev, false, None, false, gate);
        assert!(res.is_err());
        match res.unwrap_err() {
            PanteonError::Io(msg) => assert!(msg.contains("nonexistent_ms89_test_dev")),
            err => panic!("Unexpected error: {:?}", err),
        }
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
