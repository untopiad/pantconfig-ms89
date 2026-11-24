use panteon_ms89::cli::args::GateArgs;
use panteon_ms89::device::WriteGateLevel;

#[test]
fn test_gate_args_escape_hatch_triplet_enforcement() {
    // 1. allow_unverified_device alone is rejected
    let g1 = GateArgs {
        allow_unverified_device: true,
        ..Default::default()
    };
    assert!(!g1.is_unverified_escape_valid());
    assert!(g1.validate_unverified_escape().is_err());

    // 2. allow_unverified_device with only --confirm is rejected
    let g2 = GateArgs {
        allow_unverified_device: true,
        confirm: true,
        ..Default::default()
    };
    assert!(!g2.is_unverified_escape_valid());
    assert!(g2.validate_unverified_escape().is_err());

    // 3. allow_unverified_device with --write but no --confirm is rejected
    let g3 = GateArgs {
        allow_unverified_device: true,
        write: true,
        ..Default::default()
    };
    assert!(!g3.is_unverified_escape_valid());
    assert!(g3.validate_unverified_escape().is_err());

    // 4. allow_unverified_device with --confirm --write is authorized
    let g4 = GateArgs {
        allow_unverified_device: true,
        confirm: true,
        write: true,
        ..Default::default()
    };
    assert!(g4.is_unverified_escape_valid());
    assert!(g4.validate_unverified_escape().is_ok());
    assert_eq!(g4.level(), WriteGateLevel::Level2ConfigurationWrite);
    assert!(g4.authorized(WriteGateLevel::Level2ConfigurationWrite));
}

#[cfg(feature = "gui")]
#[test]
fn test_gui_check_write_permission_unverified_explanation() {
    use panteon_ms89::gui::check_write_permission;
    use panteon_ms89::model::DeviceIdentity;

    let unverified_id = DeviceIdentity {
        pid: 0x1e01,
        protocol_verified: false,
        accepted_signatures: &[0x0976, 0x0978],
    };
    let err = check_write_permission(Some(&unverified_id)).unwrap_err();
    assert!(err.contains("run verify-protocol или --allow-unverified-device в CLI"));

    let verified_id = DeviceIdentity {
        pid: 0x1440,
        protocol_verified: true,
        accepted_signatures: &[0x0976, 0x0978],
    };
    assert!(check_write_permission(Some(&verified_id)).is_ok());
}

