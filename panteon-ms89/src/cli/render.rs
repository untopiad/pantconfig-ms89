use std::path::Path;

use panteon_ms89::device::WriteGateLevel;
use panteon_ms89::codec::DeviceGetReportResponse;
use panteon_ms89::VID;

pub fn fmt_hex_slice(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
}

pub fn print_target_device(path: &Path) {
    let identity = panteon_ms89::device::resolve_device_identity(path);
    println!(
        "Target device: {} (VID 0x{:04x}, PID 0x{:04x})",
        path.display(),
        VID,
        identity.pid
    );
}

pub fn print_gate_authorized(level: WriteGateLevel, path: &Path) {
    match level {
        WriteGateLevel::Level1AddressingOnly => {
            println!(
                "[Level 1 Authorized] Opening {} under Write-Gate Level 1 (Addressing Only)...",
                path.display()
            );
        }
        WriteGateLevel::Level2ConfigurationWrite => {
            println!(
                "[Level 2 Write Authorized] Opening {} for transaction...",
                path.display()
            );
        }
        _ => {}
    }
}

pub fn print_dry_run_notice(action: &str) {
    println!(
        "\nDry-run completed. To execute {} on hardware, specify --confirm AND --unsafe-addressing (Level 1).",
        action
    );
}

pub fn print_unsafe_addressing_error(command: &str) {
    eprintln!(
        "\nError: {} requires --unsafe-addressing (Level 1 write-gate authorization).",
        command
    );
}

pub fn print_handshake(hs: &DeviceGetReportResponse) {
    println!(
        "Device handshake: state=0x{:02x}, signature=0x{:04x}, revision=0x{:02x}, status=0x{:02x}, flags=0x{:02x}",
        hs.state_byte, hs.signature, hs.revision, hs.status, hs.flags
    );
}
