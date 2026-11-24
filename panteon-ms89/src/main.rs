use std::process::ExitCode;

use clap::Parser;
use panteon_ms89::PanteonError;

use panteon_ms89::cli::*;

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {}", err);
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), PanteonError> {
    match cli.command {
        Commands::ListDevices => run_list_devices(),
        Commands::CheckAccess { print_rule } => run_check_access(print_rule),
        Commands::Probe { read, device } => run_probe(read, device),
        Commands::Export { output, profile } => run_export(output, profile),
        Commands::Import { path } => run_import(path),
        Commands::DumpState { device, state, gate } => run_dump_state(device, state, gate),
        Commands::ExperimentalReadProbe { device, gate } => run_experimental_read_probe(device, gate),
        Commands::ExperimentalRead { bank, addr, device, gate } => run_experimental_read(bank, addr, device, gate),
        Commands::ExperimentalReadPolling { device, gate } => run_experimental_read_polling(device, gate),
        Commands::ExperimentalReadSweep { base, count, device, output, gate } => run_experimental_read_sweep(base, count, device, output, gate),
        Commands::ExperimentalReadBank { bank, addr, count, device, output, gate } => run_experimental_read_bank(bank, addr, count, device, output, gate),
        Commands::DumpDescriptor { device, json, gate } => run_dump_descriptor(device, json, gate),
        Commands::SetPolling {
            rate,
            device,
            dry_run,
            settle_ms,
            commit_flash,
            get_first,
            state,
            no_ledger,
            active_slot,
            no_soft_reset,
            gate,
        } => run_set_polling(rate, device, dry_run, settle_ms, commit_flash, get_first, state, no_ledger, active_slot, no_soft_reset, gate),
        Commands::SetActiveDpi {
            slot,
            device,
            dry_run,
            settle_ms,
            commit_flash,
            state,
            no_ledger,
            no_soft_reset,
            gate,
        } => run_set_active_dpi(slot, device, dry_run, settle_ms, commit_flash, state, no_ledger, no_soft_reset, gate),
        Commands::SetFireDelay {
            delay_ms,
            device,
            dry_run,
            settle_ms,
            commit_flash,
            state,
            no_ledger,
            active_slot,
            no_soft_reset,
            gate,
        } => run_set_fire_delay(delay_ms, device, dry_run, settle_ms, commit_flash, state, no_ledger, active_slot, no_soft_reset, gate),
        Commands::SetDpiSlots {
            slot1,
            slot2,
            slot3,
            slot4,
            slot5,
            slot6,
            colors,
            brightness,
            block_session,
            device,
            dry_run,
            settle_ms,
            commit_flash,
            state,
            no_ledger,
            gate,
        } => {
            let colors_array = colors.and_then(|c| {
                if c.len() == 6 {
                    Some([c[0], c[1], c[2], c[3], c[4], c[5]])
                } else {
                    None
                }
            });
            run_set_dpi_slots(
                [slot1, slot2, slot3, slot4, slot5, slot6],
                colors_array,
                brightness,
                block_session,
                device,
                dry_run,
                settle_ms,
                commit_flash,
                state,
                no_ledger,
                gate,
            )
        }
        Commands::SetLed {
            brightness,
            effect,
            speed,
            device,
            dry_run,
            settle_ms,
            commit_flash,
            state,
            no_ledger,
            gate,
        } => run_set_led(
            brightness,
            effect,
            speed,
            device,
            dry_run,
            settle_ms,
            commit_flash,
            state,
            no_ledger,
            gate,
        ),
        Commands::SetDpiSlot {
            slot,
            dpi,
            color,
            device,
            dry_run,
            settle_ms,
            commit_flash,
            state,
            no_ledger,
            gate,
        } => run_set_dpi_slot(slot, dpi, color, device, dry_run, settle_ms, commit_flash, state, no_ledger, gate),

        Commands::VerifyProtocol { device } => run_verify_protocol(device),
        Commands::ForgetVerification { pid, state } => run_forget_verification(pid, state),
        Commands::RawFrame { hex_data, device, gate } => run_raw_frame(hex_data, device, gate),
        Commands::ProbeCommand { cmd, args, device, gate } => {
            run_probe_command(cmd, args, device, gate)
        }
        Commands::CaptureImport { pcapng, label, pid, out } => {
            run_capture_import(pcapng, label, pid, out)
        }
        Commands::PushState {
            full,
            device,
            state,
            settle_override,
            force_pid_mismatch,
            gate,
        } => run_push_state(
            full,
            device,
            state,
            settle_override,
            force_pid_mismatch,
            gate,
        ),
        Commands::RestoreState {
            snapshot,
            device,
            step,
            settle_override,
            force_pid_mismatch,
            gate,
        } => run_restore_state(
            snapshot,
            device,
            step,
            settle_override,
            force_pid_mismatch,
            gate,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_help_and_dispatch() {
        use clap::error::ErrorKind;

        let err = match Cli::try_parse_from(["panteon-ms89", "--help"]) {
            Err(e) => e,
            Ok(_) => panic!("Expected help display error"),
        };
        assert_eq!(err.kind(), ErrorKind::DisplayHelp);
        let help_text = err.to_string();

        let subcommands = [
            "list-devices",
            "probe",
            "export",
            "import",
            "dump-state",
            "experimental-read-probe",
            "experimental-read",
            "experimental-read-polling",
            "experimental-read-sweep",
            "experimental-read-bank",
            "dump-descriptor",
            "set-polling",
            "set-active-dpi",
            "set-fire-delay",
            "verify-protocol",
            "raw-frame",
            "probe-command",
            "capture-import",
            "restore-state",
        ];
        for sub in subcommands {
            assert!(help_text.contains(sub), "Missing subcommand in help: {}", sub);
        }

        let temp_profile = std::env::temp_dir().join("unit_test_panteon_profile.toml");
        let toml_data = r#"
name = "UnitTest"
id = 0
active_dpi_index = 0
polling_rate = 1000

[[dpi_levels]]
level_index = 0
dpi = 800
color_rgb = [255, 0, 0]
enabled = true

[[button_bindings]]
button_index = 0
action = "LeftClick"
"#;
        std::fs::write(&temp_profile, toml_data).unwrap();
        let temp_sweep = std::env::temp_dir().join("unit_test_panteon_sweep.json");
        let temp_sweep_str = temp_sweep.to_str().unwrap();

        let test_cases: Vec<Vec<&str>> = vec![
            vec!["panteon-ms89", "list-devices"],
            vec!["panteon-ms89", "probe", "--device", "/dev/nonexistent_ms89_test_dev"],
            vec!["panteon-ms89", "export"],
            vec!["panteon-ms89", "import", temp_profile.to_str().unwrap()],
            vec!["panteon-ms89", "dump-state", "--device", "/dev/nonexistent_ms89_test_dev"],
            vec!["panteon-ms89", "experimental-read-probe"],
            vec!["panteon-ms89", "experimental-read", "0", "0x0018"],
            vec!["panteon-ms89", "experimental-read-polling"],
            vec!["panteon-ms89", "experimental-read-sweep", "--output", temp_sweep_str],
            vec!["panteon-ms89", "experimental-read-bank", "-b", "0", "-a", "0x0018"],
            vec!["panteon-ms89", "dump-descriptor"],
            vec!["panteon-ms89", "set-polling", "1000"],
            vec!["panteon-ms89", "set-polling", "500", "--commit-flash"],
            vec!["panteon-ms89", "set-polling", "250"],
            vec!["panteon-ms89", "set-active-dpi", "2"],
            vec!["panteon-ms89", "set-active-dpi-slot", "3", "--commit-flash"],
            vec!["panteon-ms89", "set-fire-delay", "50"],
            vec!["panteon-ms89", "set-fire", "300", "--commit-flash"],
            vec!["panteon-ms89", "verify-protocol", "--device", "/dev/nonexistent_ms89_test_dev"],
            vec!["panteon-ms89", "raw-frame", "0718030001000300"],
            vec!["panteon-ms89", "probe-command", "0x0b", "01020304"],
        ];

        for args in test_cases {
            let parsed = Cli::try_parse_from(&args)
                .unwrap_or_else(|e| panic!("Failed to parse args {:?}: {}", args, e));
            let _ = run(parsed);
        }

        let _ = std::fs::remove_file(temp_profile);
    }
}
