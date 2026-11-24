use std::fs;
use std::path::{Path, PathBuf};

use crate::model::DeviceIdentity;
use crate::{PanteonError, VID};

/// Discovered device candidate info from sysfs
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredDevice {
    pub path: PathBuf,
    pub uevent_path: PathBuf,
    pub vendor_id: u16,
    pub product_id: u16,
    pub interface_number: Option<u8>,
    pub identity: DeviceIdentity,
    pub usb_bus: Option<u16>,
    pub usb_dev: Option<u16>,
}

/// Discover matching PANTEON MS89 devices by scanning /sys/class/hidraw/*/device/uevent
pub fn discover_devices() -> Result<Vec<DiscoveredDevice>, PanteonError> {
    discover_devices_in("/sys/class/hidraw", false)
}

/// Helper to select target device path: explicit path -> auto-discover -> interface 1 -> first matching device
pub fn select_device_path(device: Option<PathBuf>) -> Result<PathBuf, PanteonError> {
    if let Some(p) = device {
        return Ok(p);
    }
    let devices = discover_devices()?;
    if devices.is_empty() {
        return Err(PanteonError::DeviceNotFound);
    }
    let dev_info = devices
        .iter()
        .find(|d| d.interface_number == Some(1))
        .unwrap_or(&devices[0]);
    Ok(dev_info.path.clone())
}

/// Internal helper allowing discovery in arbitrary sysfs base directory (for testing).
/// If `group_physical` is true, interfaces belonging to the same physical USB device
/// (same vendor, product, and USB bus/dev) are deduplicated, preferring interface 1.
pub fn discover_devices_in<P: AsRef<Path>>(
    sysfs_base: P,
    group_physical: bool,
) -> Result<Vec<DiscoveredDevice>, PanteonError> {
    let base = sysfs_base.as_ref();
    if !base.exists() {
        return Ok(Vec::new());
    }

    let mut devices = Vec::new();
    let entries = fs::read_dir(base).map_err(|e| PanteonError::Io(format!("Failed to read {}: {}", base.display(), e)))?;

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let hidraw_name = file_name.to_string_lossy();
        if !hidraw_name.starts_with("hidraw") {
            continue;
        }

        let dev_path = PathBuf::from("/dev").join(&*hidraw_name);
        let uevent_path = entry.path().join("device").join("uevent");
        if !uevent_path.exists() {
            continue;
        }

        if let Ok(content) = fs::read_to_string(&uevent_path) {
            if let Some((vid, pid)) = parse_uevent_vid_pid(&content) {
                if vid == VID {
                    if let Some(identity) = DeviceIdentity::find(pid) {
                        let iface = read_interface_number(&entry.path());
                        let usb_bus_dev = parse_usb_bus_dev(&entry.path());
                        devices.push(DiscoveredDevice {
                            path: dev_path,
                            uevent_path,
                            vendor_id: vid,
                            product_id: pid,
                            interface_number: iface,
                            identity: *identity,
                            usb_bus: usb_bus_dev.map(|p| p.0),
                            usb_dev: usb_bus_dev.map(|p| p.1),
                        });
                    }
                }
            }
        }
    }

    devices.sort_by(|a, b| a.path.cmp(&b.path));

    if group_physical {
        let mut grouped: Vec<DiscoveredDevice> = Vec::new();
        for dev in devices {
            let key = (dev.vendor_id, dev.product_id, dev.usb_bus, dev.usb_dev);
            if let Some(existing) = grouped.iter_mut().find(|d| {
                (d.vendor_id, d.product_id, d.usb_bus, d.usb_dev) == key
            }) {
                if dev.interface_number == Some(1) && existing.interface_number != Some(1) {
                    *existing = dev;
                }
            } else {
                grouped.push(dev);
            }
        }
        return Ok(grouped);
    }

    Ok(devices)
}

/// Parse VID and PID from sysfs uevent content
pub fn parse_uevent_vid_pid(uevent: &str) -> Option<(u16, u16)> {
    for line in uevent.lines() {
        let line = line.trim();
        // Check HID_ID=0003:000030FA:00001440
        if let Some(rest) = line.strip_prefix("HID_ID=") {
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() == 3 {
                if let (Ok(vid), Ok(pid)) = (
                    u16::from_str_radix(parts[1], 16),
                    u16::from_str_radix(parts[2], 16),
                ) {
                    return Some((vid, pid));
                }
            }
        }
        // Check PRODUCT=3/30fa/1440/...
        if let Some(rest) = line.strip_prefix("PRODUCT=") {
            let parts: Vec<&str> = rest.split('/').collect();
            if parts.len() >= 3 {
                if let (Ok(vid), Ok(pid)) = (
                    u16::from_str_radix(parts[1], 16),
                    u16::from_str_radix(parts[2], 16),
                ) {
                    return Some((vid, pid));
                }
            }
        }
        // Check MODALIAS=hid:b0003g...v000030FAp00001440
        if let Some(rest) = line.strip_prefix("MODALIAS=") {
            if let (Some(v_pos), Some(p_pos)) = (rest.find('v'), rest.find('p')) {
                if v_pos < p_pos && p_pos + 9 <= rest.len() {
                    let v_str = &rest[v_pos + 1..p_pos];
                    let p_str = &rest[p_pos + 1..p_pos + 9];
                    if let (Ok(vid), Ok(pid)) = (
                        u16::from_str_radix(v_str, 16),
                        u16::from_str_radix(p_str, 16),
                    ) {
                        return Some((vid, pid));
                    }
                }
            }
        }
    }
    None
}

pub(crate) fn read_interface_number(hidraw_sysfs_path: &Path) -> Option<u8> {
    let candidates = [
        hidraw_sysfs_path.join("device").join("bInterfaceNumber"),
        hidraw_sysfs_path.join("device").join("..").join("bInterfaceNumber"),
    ];

    for candidate in &candidates {
        if let Ok(content) = fs::read_to_string(candidate) {
            if let Ok(num) = content.trim().parse::<u8>() {
                return Some(num);
            }
        }
    }
    None
}

/// Parse USB bus and device numbers from sysfs hidraw path (../.. relative to device)
pub fn parse_usb_bus_dev(hidraw_sysfs_path: &Path) -> Option<(u16, u16)> {
    let candidate = hidraw_sysfs_path.join("device").join("..").join("..");
    if let (Ok(bus_str), Ok(dev_str)) = (
        fs::read_to_string(candidate.join("busnum")),
        fs::read_to_string(candidate.join("devnum")),
    ) {
        if let (Ok(bus), Ok(dev)) = (bus_str.trim().parse::<u16>(), dev_str.trim().parse::<u16>()) {
            return Some((bus, dev));
        }
    }

    if let Ok(canon) = hidraw_sysfs_path.join("device").canonicalize() {
        let mut current = canon.parent();
        while let Some(dir) = current {
            if let (Ok(bus_str), Ok(dev_str)) = (
                fs::read_to_string(dir.join("busnum")),
                fs::read_to_string(dir.join("devnum")),
            ) {
                if let (Ok(bus), Ok(dev)) = (bus_str.trim().parse::<u16>(), dev_str.trim().parse::<u16>()) {
                    return Some((bus, dev));
                }
            }
            current = dir.parent();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_usb_bus_dev_synthetic() {
        let temp_dir = std::env::temp_dir().join(format!("panteon_test_bus_dev_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);

        let usb_dev_dir = temp_dir.join("usb1").join("1-4");
        fs::create_dir_all(&usb_dev_dir).unwrap();
        fs::write(usb_dev_dir.join("busnum"), "1\n").unwrap();
        fs::write(usb_dev_dir.join("devnum"), "37\n").unwrap();

        let usb_iface_dir = usb_dev_dir.join("1-4:1.1");
        fs::create_dir_all(&usb_iface_dir).unwrap();

        let hidraw_device_dir = usb_iface_dir.join("0003:30FA:1440.0001");
        fs::create_dir_all(&hidraw_device_dir).unwrap();

        let hidraw_sysfs = temp_dir.join("hidraw1");
        fs::create_dir_all(&hidraw_sysfs).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let _ = symlink(&hidraw_device_dir, hidraw_sysfs.join("device"));
        }
        #[cfg(not(unix))]
        {
            let _ = fs::create_dir_all(hidraw_sysfs.join("device"));
        }

        let parsed = parse_usb_bus_dev(&hidraw_sysfs);
        assert_eq!(parsed, Some((1, 37)));

        let _ = fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_parse_uevent_hid_id() {
        let content = "DEVNAME=hidraw0\nHID_ID=0003:000030FA:00001440\nHID_NAME=INSTANT USB GAMING MOUSE\n";
        let parsed = parse_uevent_vid_pid(content);
        assert_eq!(parsed, Some((0x30fa, 0x1440)));
    }

    #[test]
    fn test_parse_uevent_product() {
        let content = "PRODUCT=3/30fa/1440/100\n";
        let parsed = parse_uevent_vid_pid(content);
        assert_eq!(parsed, Some((0x30fa, 0x1440)));
    }

    #[test]
    fn test_parse_uevent_non_matching() {
        let content = "HID_ID=0003:00001234:00005678\n";
        let parsed = parse_uevent_vid_pid(content);
        assert_eq!(parsed, Some((0x1234, 0x5678)));
    }

    #[test]
    fn test_select_device_path_explicit() {
        let explicit = PathBuf::from("/dev/hidraw42");
        let result = select_device_path(Some(explicit.clone())).unwrap();
        assert_eq!(result, explicit);
    }

    #[test]
    fn test_discover_devices_identity_filtering() {
        let temp_dir = std::env::temp_dir().join(format!("panteon_test_sysfs_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);

        let hidraw1_dir = temp_dir.join("hidraw1").join("device");
        fs::create_dir_all(&hidraw1_dir).unwrap();
        fs::write(
            hidraw1_dir.join("uevent"),
            "HID_ID=0003:000030FA:00001E01\nHID_NAME=INSTANT USB GAMING MOUSE\n",
        ).unwrap();

        let hidraw2_dir = temp_dir.join("hidraw2").join("device");
        fs::create_dir_all(&hidraw2_dir).unwrap();
        fs::write(
            hidraw2_dir.join("uevent"),
            "HID_ID=0003:000030FA:00009999\nHID_NAME=UNKNOWN MOUSE\n",
        ).unwrap();

        let discovered = discover_devices_in(&temp_dir, false).unwrap();
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].product_id, 0x1e01);
        assert!(!discovered[0].identity.protocol_verified);
        assert_eq!(discovered[0].identity.accepted_signatures, &[0x0976, 0x0978]);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_discover_devices_grouped_physical() {
        let temp_dir = std::env::temp_dir().join(format!("panteon_test_grouped_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);

        let usb_dev_dir = temp_dir.join("usb1").join("1-4");
        fs::create_dir_all(&usb_dev_dir).unwrap();
        fs::write(usb_dev_dir.join("busnum"), "1\n").unwrap();
        fs::write(usb_dev_dir.join("devnum"), "37\n").unwrap();

        // Interface 0
        let iface0_dir = usb_dev_dir.join("1-4:1.0");
        let dev0_dir = iface0_dir.join("0003:30FA:1440.0001");
        fs::create_dir_all(&dev0_dir).unwrap();
        fs::write(dev0_dir.join("uevent"), "HID_ID=0003:000030FA:00001440\nHID_NAME=PANTEON MS89\n").unwrap();
        fs::write(dev0_dir.join("bInterfaceNumber"), "0\n").unwrap();

        let hidraw1_sysfs = temp_dir.join("hidraw1");
        fs::create_dir_all(&hidraw1_sysfs).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let _ = symlink(&dev0_dir, hidraw1_sysfs.join("device"));
        }
        #[cfg(not(unix))]
        {
            let _ = fs::create_dir_all(hidraw1_sysfs.join("device"));
        }

        // Interface 1
        let iface1_dir = usb_dev_dir.join("1-4:1.1");
        let dev1_dir = iface1_dir.join("0003:30FA:1440.0002");
        fs::create_dir_all(&dev1_dir).unwrap();
        fs::write(dev1_dir.join("uevent"), "HID_ID=0003:000030FA:00001440\nHID_NAME=PANTEON MS89\n").unwrap();
        fs::write(dev1_dir.join("bInterfaceNumber"), "1\n").unwrap();

        let hidraw2_sysfs = temp_dir.join("hidraw2");
        fs::create_dir_all(&hidraw2_sysfs).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let _ = symlink(&dev1_dir, hidraw2_sysfs.join("device"));
        }
        #[cfg(not(unix))]
        {
            let _ = fs::create_dir_all(hidraw2_sysfs.join("device"));
        }

        // CLI path (group_physical = false): sees both interfaces
        let raw_devs = discover_devices_in(&temp_dir, false).unwrap();
        assert_eq!(raw_devs.len(), 2);

        // Grouped path (group_physical = true): grouped into 1 physical device
        let grouped_devs = discover_devices_in(&temp_dir, true).unwrap();
        assert_eq!(grouped_devs.len(), 1);
        assert_eq!(grouped_devs[0].usb_bus, Some(1));
        assert_eq!(grouped_devs[0].usb_dev, Some(37));
        assert_eq!(grouped_devs[0].interface_number, Some(1));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
