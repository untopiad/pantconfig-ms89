use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{DeviceIdentity, KNOWN_IDENTITIES};
use crate::{FEATURE_REPORT_LEN, REPORT_ID, VID};
use super::discovery::parse_uevent_vid_pid;

/// Determine device identity from sysfs uevent based on hidraw device path
pub fn resolve_device_identity<P: AsRef<Path>>(device_path: P) -> DeviceIdentity {
    let p = device_path.as_ref();
    if let Some(file_name) = p.file_name() {
        let uevent_path = PathBuf::from("/sys/class/hidraw")
            .join(file_name)
            .join("device")
            .join("uevent");
        if let Ok(content) = fs::read_to_string(&uevent_path) {
            if let Some((vid, pid)) = parse_uevent_vid_pid(&content) {
                if vid == VID {
                    if let Some(known) = DeviceIdentity::find(pid) {
                        return *known;
                    } else {
                        eprintln!(
                            "Warning: device PID 0x{:04x} is unknown to identity registry (read permitted, Level 2 writes blocked)",
                            pid
                        );
                        return DeviceIdentity {
                            pid,
                            protocol_verified: false,
                            accepted_signatures: &[],
                        };
                    }
                }
            }
        }
    }
    // Fallback if uevent is not readable or in unit tests: default to primary identity
    KNOWN_IDENTITIES[0]
}

/// Check if a persisted verification marker exists for PID in specified file and signature matches
pub fn check_persisted_verification_with_marker_file(
    marker_path: &Path,
    pid: u16,
    signature: u16,
) -> bool {
    if let Ok(store) = crate::store::load_verified_store(marker_path) {
        if let Some(entry) = store.find(pid) {
            if entry.signatures.contains(&signature) {
                println!(
                    "verified by persisted marker for PID 0x{:04x} (signature 0x{:04x})",
                    pid, signature
                );
                return true;
            } else {
                eprintln!(
                    "Warning: persisted marker found for PID 0x{:04x}, but actual signature 0x{:04x} not in marker signatures {:04x?}; marker ignored",
                    pid, signature, entry.signatures
                );
            }
        }
    }
    false
}

/// Check if a persisted verification marker exists for PID in default location and signature matches
pub fn check_persisted_verification_with_signature(pid: u16, signature: u16) -> bool {
    let marker_path = crate::store::resolve_verified_marker_path(None);
    check_persisted_verification_with_marker_file(&marker_path, pid, signature)
}

/// Validate an ioctl GET_REPORT frame against persisted verification marker, updating `identity.protocol_verified` if matched.
/// Returns true if verified by marker.
pub fn check_marker_verification_report(
    identity: &mut DeviceIdentity,
    report: &[u8; FEATURE_REPORT_LEN],
    marker_path: Option<&Path>,
) -> bool {
    if report[0] != REPORT_ID {
        eprintln!(
            "[device::open] Verification marker check: report[0] != REPORT_ID (got 0x{:02x}, expected 0x{:02x}), raw=[{}]",
            report[0],
            REPORT_ID,
            crate::codec::hex_format(report)
        );
        return false;
    }
    let actual_sig = u16::from_le_bytes([report[2], report[3]]);
    let verified = match marker_path {
        Some(path) => check_persisted_verification_with_marker_file(path, identity.pid, actual_sig),
        None => check_persisted_verification_with_signature(identity.pid, actual_sig),
    };
    if verified {
        identity.protocol_verified = true;
    }
    verified
}

