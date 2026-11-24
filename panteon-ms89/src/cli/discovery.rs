use std::path::PathBuf;

use panteon_ms89::codec::DeviceGetReportResponse;
use panteon_ms89::device::{select_device_path, Device, Transport};
use panteon_ms89::service::discover as discover_devices;
use panteon_ms89::model::KNOWN_IDENTITIES;
use panteon_ms89::{PanteonError, FEATURE_REPORT_LEN, VID};

use super::render::{fmt_hex_slice, print_handshake, print_target_device};

pub fn run_list_devices() -> Result<(), PanteonError> {
    let devices = discover_devices()?;
    if devices.is_empty() {
        let pids_str = KNOWN_IDENTITIES
            .iter()
            .map(|id| format!("0x{:04x}", id.pid))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "No matching PANTEON MS89 devices found (VID 0x{:04x}, known PIDs: {}).",
            VID, pids_str
        );
        println!("Ensure the mouse is connected and udev rules are loaded.");
    } else {
        println!("Found {} matching device(s):", devices.len());
        for dev in &devices {
            let iface_str = match dev.interface_number {
                Some(iface) => format!("interface {}", iface),
                None => "unknown interface".to_string(),
            };
            let protocol_str = if dev.identity.protocol_verified {
                "verified"
            } else {
                "unverified"
            };
            println!(
                "  - {} (PID 0x{:04x}, {}, protocol: {}, sysfs: {})",
                dev.path.display(),
                dev.product_id,
                iface_str,
                protocol_str,
                dev.uevent_path.display()
            );
        }
    }
    Ok(())
}

pub fn run_probe(read: bool, device: Option<PathBuf>) -> Result<(), PanteonError> {
    let _ = read;
    let target_path = select_device_path(device)?;

    print_target_device(&target_path);

    println!("Opening {} in read-only mode...", target_path.display());
    let mut dev = Device::open(&target_path)?;

    let mut raw_report = [0u8; FEATURE_REPORT_LEN];
    dev.read_feature(&mut raw_report)?;

    let raw_state = raw_report[1];
    let raw_sig = if raw_report.len() >= 4 {
        u16::from_le_bytes([raw_report[2], raw_report[3]])
    } else {
        0
    };
    let hex_str = fmt_hex_slice(&raw_report);

    println!(
        "Probe response: state=0x{:02x}, signature=0x{:04x}, raw=[{}]",
        raw_state, raw_sig, hex_str
    );

    match DeviceGetReportResponse::decode_with_expected_signatures(&raw_report, dev.identity().accepted_signatures) {
        Ok(hs) => {
            print_handshake(&hs);
            if dev.identity().accepts_signature(hs.signature) {
                println!(
                    "  [Identity Check] PID 0x{:04x}: signature 0x{:04x} matches identity accepted signatures {:04x?} (state 0x{:02x}, revision 0x{:02x}, status 0x{:02x})",
                    dev.identity().pid, hs.signature, dev.identity().accepted_signatures, hs.state_byte, hs.revision, hs.status
                );
            } else {
                println!(
                    "  [Identity Check] Warning: PID 0x{:04x} signature 0x{:04x} NOT in accepted signatures {:04x?} (state 0x{:02x})",
                    dev.identity().pid, hs.signature, dev.identity().accepted_signatures, hs.state_byte
                );
            }
        }
        Err(err) => {
            println!("  [Decode Error] Failed to decode handshake: {}", err);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_device_path_explicit() {
        let explicit = PathBuf::from("/dev/hidraw88");
        let selected = select_device_path(Some(explicit.clone())).unwrap();
        assert_eq!(selected, explicit);
    }

    #[test]
    fn test_probe_handshake_state_0xff_decodes_cleanly() {
        let raw = [0x07, 0xff, 0x76, 0x09, 0x1e, 0x00, 0x23, 0x7b];
        let accepted = [0x0976, 0x0978];
        let hs = DeviceGetReportResponse::decode_with_expected_signatures(&raw, &accepted)
            .expect("probe frame with state 0xff must decode without error");
        assert_eq!(hs.state_byte, 0xff);
        assert_eq!(hs.signature, 0x0976);
        assert_eq!(hs.revision, 0x1e);
        assert_eq!(hs.status, 0x23);
        assert_eq!(hs.flags, 0x7b);
    }
}
