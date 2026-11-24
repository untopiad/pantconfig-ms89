//! Read-only diagnostics for device access (udev rule + node permissions).
//!
//! This module answers a single question before any hardware I/O: *is the host
//! configured so the user can open both relevant device nodes?* It never issues
//! an ioctl and never writes to a device: nodes are opened read+write and closed
//! immediately, the udev rule is only read, and sysfs was already read by
//! [`super::discovery`].
//!
//! Access model (confirmed on the reference machine):
//! - `/dev/hidrawN` carries the HID feature reports (configuration read/write).
//! - `/dev/bus/usb/{bus}/{dev}` is needed for `USBDEVFS_RESET` (re-enumeration
//!   after a polling-rate change); the `input` group does **not** cover it.
//! - The shipped `99-panteon-ms89.rules` (`MODE="0660", TAG+="uaccess"`) grants
//!   the active logind session an ACL on both nodes; no extra group is needed.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::discovery::DiscoveredDevice;

/// Exact contents of the udev rule shipped in `udev/99-panteon-ms89.rules`.
pub const UDEV_RULE: &str = include_str!("../../udev/99-panteon-ms89.rules");
/// File name the rule must have under one of the udev rule directories.
pub const RULE_NAME: &str = "99-panteon-ms89.rules";

/// Environment variable overriding the rule search path for diagnostics
/// (colon-separated). Intended for tests and screenshots; production behaviour
/// uses [`default_rule_dirs`].
pub const RULE_DIRS_ENV: &str = "PANTEON_UDEV_RULE_DIRS";

/// State of the shipped udev rule on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleState {
    /// No `99-panteon-ms89.rules` in any of the searched directories.
    Missing,
    /// The file exists but its contents differ from [`UDEV_RULE`].
    Outdated,
    /// The file exists and its contents are byte-identical to [`UDEV_RULE`].
    Installed,
}

/// Result of trying to open a device node read+write (no ioctl is performed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeAccess {
    /// The node exists and the current user can open it read+write.
    Ok,
    /// Permission denied (or an unexpected open error — classified conservatively).
    Denied,
    /// The node does not exist / no device is connected.
    NoNode,
}

/// Snapshot of the access situation for the first discovered device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessReport {
    pub rule: RuleState,
    /// Directory where the rule was found (for `Outdated`/`Installed`).
    pub rule_path: Option<PathBuf>,
    /// Access to the first discovered device's `/dev/hidrawN`.
    pub hidraw: NodeAccess,
    /// Access to `/dev/bus/usb/{bus:03}/{dev:03}` for `USBDEVFS_RESET`.
    pub usb_reset: NodeAccess,
}

/// Standard udev rule directories, in search order.
pub fn default_rule_dirs() -> Vec<PathBuf> {
    [
        "/etc/udev/rules.d",
        "/usr/lib/udev/rules.d",
        "/lib/udev/rules.d",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

/// Rule directories from [`RULE_DIRS_ENV`], if set and non-empty.
pub fn rule_dirs_from_env() -> Option<Vec<PathBuf>> {
    let raw = std::env::var_os(RULE_DIRS_ENV)?;
    let dirs: Vec<PathBuf> = std::env::split_paths(&raw)
        .filter(|p| !p.as_os_str().is_empty())
        .collect();
    if dirs.is_empty() {
        None
    } else {
        Some(dirs)
    }
}

/// Diagnose using the standard directories (or [`RULE_DIRS_ENV`] override).
pub fn diagnose(devices: &[DiscoveredDevice]) -> AccessReport {
    let dirs = rule_dirs_from_env().unwrap_or_else(default_rule_dirs);
    diagnose_with_paths(devices, &dirs)
}

/// Pure, testable diagnosis: rule directories are supplied by the caller.
pub fn diagnose_with_paths(devices: &[DiscoveredDevice], rule_dirs: &[PathBuf]) -> AccessReport {
    let (rule, rule_path) = find_rule(rule_dirs);
    let target = target_device(devices);

    let hidraw = target
        .map(|d| check_node_access(&d.path))
        .unwrap_or(NodeAccess::NoNode);

    let usb_reset = target
        .and_then(|d| match (d.usb_bus, d.usb_dev) {
            (Some(bus), Some(dev)) => Some(usb_reset_path(bus, dev)),
            _ => None,
        })
        .map(|path| check_node_access(&path))
        .unwrap_or(NodeAccess::NoNode);

    AccessReport {
        rule,
        rule_path,
        hidraw,
        usb_reset,
    }
}

/// First discovered device, preferring interface 1 (the configuration interface).
fn target_device(devices: &[DiscoveredDevice]) -> Option<&DiscoveredDevice> {
    devices
        .iter()
        .find(|d| d.interface_number == Some(1))
        .or_else(|| devices.first())
}

fn find_rule(rule_dirs: &[PathBuf]) -> (RuleState, Option<PathBuf>) {
    for dir in rule_dirs {
        let path = dir.join(RULE_NAME);
        match fs::read_to_string(&path) {
            Ok(contents) => {
                let state = if contents == UDEV_RULE {
                    RuleState::Installed
                } else {
                    RuleState::Outdated
                };
                return (state, Some(path));
            }
            // File absent (or directory missing): keep searching.
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            // Present but unreadable: treat as outdated, conservatively.
            Err(_) => return (RuleState::Outdated, Some(path)),
        }
    }
    (RuleState::Missing, None)
}

/// `USBDEVFS_RESET` node path with zero-padded 3-digit bus/device numbers.
pub fn usb_reset_path(bus: u16, dev: u16) -> PathBuf {
    PathBuf::from(format!("/dev/bus/usb/{:03}/{:03}", bus, dev))
}

/// Map an `open(2)` error kind onto the access classification.
pub fn classify_open_error(kind: io::ErrorKind) -> NodeAccess {
    match kind {
        io::ErrorKind::NotFound => NodeAccess::NoNode,
        io::ErrorKind::PermissionDenied => NodeAccess::Denied,
        // Conservative: anything unexpected is reported as no access.
        _ => NodeAccess::Denied,
    }
}

/// Open a node read+write and close it immediately. No ioctl, no read, no write.
pub fn check_node_access(path: &Path) -> NodeAccess {
    match fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
    {
        Ok(file) => {
            drop(file);
            NodeAccess::Ok
        }
        Err(err) => classify_open_error(err.kind()),
    }
}

/// Self-contained, offline installation command (heredoc; no network).
pub fn install_command() -> String {
    format!(
        "sudo tee /etc/udev/rules.d/{RULE_NAME} > /dev/null <<'EOF'\n\
         {rule}EOF\n\
         sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=hidraw --subsystem-match=usb\n",
        rule = UDEV_RULE
    )
}

/// `true` when the rule is installed and neither node is denied.
///
/// A missing device (`NoNode`) does not fail the report: the rule is still valid
/// when the mouse is simply unplugged.
pub fn report_is_ok(report: &AccessReport) -> bool {
    report.rule == RuleState::Installed
        && report.hidraw != NodeAccess::Denied
        && report.usb_reset != NodeAccess::Denied
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "panteon_access_{}_{}_{}",
                tag,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fake_device() -> DiscoveredDevice {
        DiscoveredDevice {
            path: PathBuf::from("/dev/panteon-does-not-exist"),
            uevent_path: PathBuf::from("/sys/fake/uevent"),
            vendor_id: crate::VID,
            product_id: crate::PID,
            interface_number: Some(1),
            identity: crate::model::DeviceIdentity::unknown(),
            usb_bus: Some(3),
            usb_dev: Some(7),
        }
    }

    #[test]
    fn rule_state_missing_outdated_installed() {
        let dir = TempDir::new("rule_states");

        let (state, path) = find_rule(&[dir.path().to_path_buf()]);
        assert_eq!(state, RuleState::Missing);
        assert!(path.is_none());

        let rule_file = dir.path().join(RULE_NAME);
        fs::write(&rule_file, "SUBSYSTEM==\"hidraw\", MODE=\"0660\"\n").unwrap();
        let (state, path) = find_rule(&[dir.path().to_path_buf()]);
        assert_eq!(state, RuleState::Outdated);
        assert_eq!(path.as_deref(), Some(rule_file.as_path()));

        fs::write(&rule_file, UDEV_RULE).unwrap();
        let (state, path) = find_rule(&[dir.path().to_path_buf()]);
        assert_eq!(state, RuleState::Installed);
        assert_eq!(path.as_deref(), Some(rule_file.as_path()));
    }

    #[test]
    fn rule_search_prefers_first_directory_with_file() {
        let first = TempDir::new("rule_first");
        let second = TempDir::new("rule_second");
        fs::write(second.path().join(RULE_NAME), UDEV_RULE).unwrap();
        fs::write(first.path().join(RULE_NAME), "different\n").unwrap();

        let (state, path) = find_rule(&[first.path().to_path_buf(), second.path().to_path_buf()]);
        assert_eq!(state, RuleState::Outdated);
        assert_eq!(path.as_deref(), Some(first.path().join(RULE_NAME).as_path()));
    }

    #[test]
    fn usb_reset_path_is_zero_padded() {
        assert_eq!(
            usb_reset_path(3, 7),
            PathBuf::from("/dev/bus/usb/003/007")
        );
        assert_eq!(
            usb_reset_path(1, 42),
            PathBuf::from("/dev/bus/usb/001/042")
        );
        assert_eq!(
            usb_reset_path(12, 250),
            PathBuf::from("/dev/bus/usb/012/250")
        );
    }

    #[test]
    fn classify_open_error_matches_contract() {
        assert_eq!(
            classify_open_error(io::ErrorKind::PermissionDenied),
            NodeAccess::Denied
        );
        assert_eq!(
            classify_open_error(io::ErrorKind::NotFound),
            NodeAccess::NoNode
        );
        assert_eq!(
            classify_open_error(io::ErrorKind::Other),
            NodeAccess::Denied,
            "unexpected open errors must be reported conservatively"
        );
    }

    #[test]
    fn check_node_access_existing_file_is_ok_and_missing_is_no_node() {
        let dir = TempDir::new("node");
        let file = dir.path().join("node");
        fs::write(&file, b"x").unwrap();
        assert_eq!(check_node_access(&file), NodeAccess::Ok);
        assert_eq!(
            check_node_access(&dir.path().join("absent")),
            NodeAccess::NoNode
        );
    }

    #[test]
    fn report_is_ok_matrix() {
        let rules = [
            RuleState::Missing,
            RuleState::Outdated,
            RuleState::Installed,
        ];
        let nodes = [NodeAccess::Ok, NodeAccess::Denied, NodeAccess::NoNode];

        for rule in rules {
            for hidraw in nodes {
                for usb_reset in nodes {
                    let report = AccessReport {
                        rule,
                        rule_path: None,
                        hidraw,
                        usb_reset,
                    };
                    let expected = rule == RuleState::Installed
                        && hidraw != NodeAccess::Denied
                        && usb_reset != NodeAccess::Denied;
                    assert_eq!(
                        report_is_ok(&report),
                        expected,
                        "rule={rule:?} hidraw={hidraw:?} usb={usb_reset:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn install_command_is_self_contained() {
        let cmd = install_command();
        assert!(cmd.contains(&format!("sudo tee /etc/udev/rules.d/{RULE_NAME}")));
        assert!(cmd.contains("<<'EOF'"));
        assert!(
            cmd.contains(UDEV_RULE.trim_end()),
            "the full rule body must be embedded verbatim"
        );
        assert!(cmd.contains("sudo udevadm control --reload-rules"));
        assert!(cmd.contains(
            "sudo udevadm trigger --subsystem-match=hidraw --subsystem-match=usb"
        ));
    }

    #[test]
    fn diagnose_without_devices_reports_no_node_and_checks_rule() {
        let dir = TempDir::new("no_devices");
        fs::write(dir.path().join(RULE_NAME), UDEV_RULE).unwrap();

        let report = diagnose_with_paths(&[], &[dir.path().to_path_buf()]);
        assert_eq!(report.rule, RuleState::Installed);
        assert_eq!(report.hidraw, NodeAccess::NoNode);
        assert_eq!(report.usb_reset, NodeAccess::NoNode);
        assert!(
            report_is_ok(&report),
            "installed rule + no device connected must be OK"
        );
    }

    #[test]
    fn diagnose_uses_device_bus_and_dev_for_usb_path() {
        let dir = TempDir::new("device_paths");
        let device = fake_device();

        let report = diagnose_with_paths(&[device], &[dir.path().to_path_buf()]);
        assert_eq!(report.rule, RuleState::Missing);
        assert_eq!(report.hidraw, NodeAccess::NoNode);
        assert_eq!(
            report.usb_reset,
            NodeAccess::NoNode,
            "path /dev/bus/usb/003/007 is derived from usb_bus/usb_dev and is absent here"
        );
    }

    #[test]
    fn rule_dirs_from_env_is_parsed_as_path_list() {
        // `split_paths` on ':' is static behaviour; no process env mutation here.
        let parsed: Vec<PathBuf> =
            std::env::split_paths(std::ffi::OsStr::new("/tmp/a:/tmp/b")).collect();
        assert_eq!(parsed, vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]);
    }
}