use std::process::Command;

#[test]
fn run_help_and_dispatch() {
    let bin = env!("CARGO_BIN_EXE_panteon-ms89");

    // 1. Check --help output contains all commands
    let help_output = Command::new(bin)
        .arg("--help")
        .output()
        .expect("Failed to execute panteon-ms89 --help");
    assert!(help_output.status.success());
    let help_str = String::from_utf8_lossy(&help_output.stdout);

    let expected_commands = [
        "list-devices",
        "check-access",
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
        "capture-import",
        "restore-state",
        "help",
    ];

    for cmd in expected_commands {
        assert!(
            help_str.contains(cmd),
            "Help output missing subcommand: {}",
            cmd
        );
    }

    // 2. Test dry-run dispatch of all subcommands without panics
    let temp_profile = std::env::temp_dir().join("test_panteon_profile_smoke.toml");
    let sample_toml = r#"
name = "TestConfig"
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
    std::fs::write(&temp_profile, sample_toml).expect("Failed to write temp profile");
    let temp_sweep = std::env::temp_dir().join(format!(
        "test_panteon_sweep_{}.json",
        std::process::id()
    ));
    let temp_sweep_str = temp_sweep.to_str().unwrap();

    let dry_run_invocations: Vec<Vec<&str>> = vec![
        vec!["list-devices"],
        vec!["probe", "--device", "/dev/nonexistent_ms89_test_dev"],
        vec!["export"],
        vec!["import", temp_profile.to_str().unwrap()],
        vec!["dump-state", "--device", "/dev/nonexistent_ms89_test_dev"],
        vec!["experimental-read-probe"],
        vec!["experimental-read", "0", "0x0018"],
        vec!["experimental-read-polling"],
        vec!["experimental-read-sweep", "--output", temp_sweep_str],
        vec!["experimental-read-bank", "-b", "0", "-a", "0x0018"],
        vec!["dump-descriptor"],
        vec!["set-polling", "1000"],
        vec!["set-polling", "250"],
        vec!["set-fire-delay", "50"],
        vec!["raw-frame", "0718030001000300"],
    ];

    for args in dry_run_invocations {
        let output = Command::new(bin)
            .args(&args)
            .output()
            .unwrap_or_else(|e| panic!("Failed to run with args {:?}: {}", args, e));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("panicked at"),
            "Command with args {:?} panicked: stderr: {}",
            args,
            stderr
        );
    }

    let _ = std::fs::remove_file(temp_profile);
    let _ = std::fs::remove_file(temp_sweep);
}

#[test]
fn experimental_read_sweep_requires_output() {
    use clap::Parser;
    use panteon_ms89::cli::Cli;

    let err = Cli::try_parse_from(["panteon-ms89", "experimental-read-sweep"])
        .err()
        .expect("--output must be required");
    assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
}

#[test]
fn check_access_parses() {
    use clap::Parser;
    use panteon_ms89::cli::{Cli, Commands};

    let cli = Cli::try_parse_from(["panteon-ms89", "check-access"]).unwrap();
    assert!(matches!(
        cli.command,
        Commands::CheckAccess { print_rule: false }
    ));

    let cli = Cli::try_parse_from(["panteon-ms89", "check-access", "--print-rule"]).unwrap();
    assert!(matches!(
        cli.command,
        Commands::CheckAccess { print_rule: true }
    ));
}

#[test]
fn check_access_print_rule_is_byte_identical_to_udev_file() {
    let bin = env!("CARGO_BIN_EXE_panteon-ms89");
    let output = Command::new(bin)
        .args(["check-access", "--print-rule"])
        .output()
        .expect("Failed to execute panteon-ms89 check-access --print-rule");
    assert!(output.status.success());

    let rule_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("udev")
        .join("99-panteon-ms89.rules");
    let expected = std::fs::read(&rule_path).expect("read udev rule");
    assert_eq!(output.stdout, expected, "--print-rule must be byte-identical");
}

#[test]
fn check_access_without_device_reports_install_command() {
    let bin = env!("CARGO_BIN_EXE_panteon-ms89");
    let empty_dir = std::env::temp_dir().join(format!(
        "panteon_cli_access_empty_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&empty_dir).unwrap();

    let output = Command::new(bin)
        .arg("check-access")
        .env("PANTEON_UDEV_RULE_DIRS", &empty_dir)
        .output()
        .expect("Failed to execute panteon-ms89 check-access");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!stderr.contains("panicked at"), "check-access panicked: {}", stderr);
    assert_eq!(output.status.code(), Some(1), "missing rule must exit with 1");
    assert!(
        stdout.contains("udev-правило: отсутствует"),
        "missing rule line, got: {}",
        stdout
    );
    assert!(
        stdout.contains("sudo tee /etc/udev/rules.d/99-panteon-ms89.rules"),
        "install command must be printed, got: {}",
        stdout
    );
    assert!(
        stdout.contains("udevadm control --reload-rules"),
        "reload command must be printed"
    );

    let _ = std::fs::remove_dir_all(&empty_dir);
}
