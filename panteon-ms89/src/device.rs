pub mod access;
pub mod discovery;
pub mod gate;
pub mod identity;
pub mod read;
pub mod transaction;

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use access::{
    check_node_access, classify_open_error, default_rule_dirs, diagnose, diagnose_with_paths,
    install_command, report_is_ok, rule_dirs_from_env, usb_reset_path, AccessReport, NodeAccess,
    RuleState, RULE_DIRS_ENV, RULE_NAME, UDEV_RULE,
};
pub use discovery::{
    discover_devices, discover_devices_in, parse_uevent_vid_pid, select_device_path,
    DiscoveredDevice,
};
pub use gate::WriteGateLevel;
pub use identity::{
    check_marker_verification_report, check_persisted_verification_with_marker_file,
    check_persisted_verification_with_signature, resolve_device_identity,
};
pub use transaction::{BlockWrite, WriteTransactionPlan};

use crate::codec::DescriptorPage;
use crate::model::{DeviceIdentity, DeviceState};
use crate::{PanteonError, FEATURE_REPORT_LEN};

/// Hardware transport interface for PANTEON MS89
pub trait Transport
{
    fn read_feature(&mut self, report: &mut [u8; FEATURE_REPORT_LEN]) -> Result<(), PanteonError>;
    fn write_gate_level(&self) -> WriteGateLevel { WriteGateLevel::Level0ReadOnly }
    fn set_write_gate_level(&mut self, _level: WriteGateLevel) {}
    fn device_identity(&self) -> DeviceIdentity { DeviceIdentity::unknown() }

    fn write_feature(&mut self, report: &[u8; FEATURE_REPORT_LEN]) -> Result<(), PanteonError> {
        gate::check_write_gate(self.write_gate_level(), &self.device_identity(), report)?;
        self.raw_write_feature(report)
    }

    fn raw_write_feature(&mut self, _report: &[u8; FEATURE_REPORT_LEN]) -> Result<(), PanteonError> {
        Err(PanteonError::WriteDisabled)
    }

    fn execute_transaction(&mut self, plan: &WriteTransactionPlan, dry_run: bool) -> Result<(), PanteonError> {
        transaction::execute_transaction(self, plan, dry_run, 0)
    }

    fn execute_transaction_with_settle(&mut self, plan: &WriteTransactionPlan, dry_run: bool, settle_ms: u64) -> Result<(), PanteonError> {
        transaction::execute_transaction(self, plan, dry_run, settle_ms)
    }

    fn rollback_transaction(&mut self, plan: &WriteTransactionPlan) {
        transaction::rollback_transaction(self, plan);
    }

    fn read_state(&mut self) -> Result<DeviceState, PanteonError> { read::read_state(self) }
    fn read_register(&mut self, bank: u8, addr: u16) -> Result<u16, PanteonError> { read::read_register(self, bank, addr) }
    fn experimental_read_register(&mut self, bank: u8, addr: u16) -> Result<[u8; 7], PanteonError> { read::experimental_read_register(self, bank, addr) }
    fn experimental_read_byte(&mut self, addr: u16) -> Result<u8, PanteonError> { read::experimental_read_byte(self, addr) }
    fn experimental_read(&mut self, bank: u8, base: u16, count: usize, delay: Duration) -> Result<Vec<u8>, PanteonError> {
        read::experimental_read(self, bank, base, count, delay)
    }
    fn experimental_read_chunk(&mut self, base: u16, count: usize) -> Result<Vec<u8>, PanteonError> {
        read::experimental_read_chunk(self, base, count)
    }
    fn experimental_read_bank(&mut self, bank: u8, addr: u16, count: usize) -> Result<Vec<u8>, PanteonError> {
        read::experimental_read_bank(self, bank, addr, count)
    }
    fn read_descriptor_page(&mut self) -> Result<DescriptorPage, PanteonError> {
        read::read_descriptor_page(self)
    }
}

/// Linux userspace handle for PANTEON MS89 USB mouse (hidraw interface)
pub struct Device {
    #[allow(dead_code)]
    file: File,
    path: PathBuf,
    gate_level: WriteGateLevel,
    identity: DeviceIdentity,
}

impl Device {
    pub const READ_PROTOCOL_DELAY: Duration = Duration::from_millis(2);

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, PanteonError> {
        Self::open_with_gate_level(path, WriteGateLevel::Level0ReadOnly)
    }

    pub fn open_with_gate_level<P: AsRef<Path>>(path: P, gate_level: WriteGateLevel) -> Result<Self, PanteonError> {
        let path_buf = path.as_ref().to_path_buf();
        let identity = resolve_device_identity(&path_buf);
        Self::open_with_gate_and_identity(path_buf, gate_level, identity)
    }

    pub fn open_with_gate_and_identity<P: AsRef<Path>>(
        path: P,
        gate_level: WriteGateLevel,
        mut identity: DeviceIdentity,
    ) -> Result<Self, PanteonError> {
        let path_buf = path.as_ref().to_path_buf();
        let write_needed = gate_level >= WriteGateLevel::Level1AddressingOnly;
        let file = OpenOptions::new()
            .read(true)
            .write(write_needed)
            .open(&path_buf)
            .map_err(|e| PanteonError::Io(format!("Failed to open {}: {}", path_buf.display(), e)))?;

        // Level >= 1: if identity is unverified in code, check persisted verification marker with signature match
        if gate_level >= WriteGateLevel::Level1AddressingOnly && !identity.protocol_verified {
            let mut report = [0u8; FEATURE_REPORT_LEN];
            match read::read_feature_ioctl(&file, &mut report) {
                Ok(_) => {
                    check_marker_verification_report(&mut identity, &report, None);
                }
                Err(e) => {
                    eprintln!(
                        "[device::open] Verification marker check: ioctl read failed: {}, raw=[{}]",
                        e,
                        crate::codec::hex_format(&report)
                    );
                }
            }
        }

        Ok(Self { file, path: path_buf, gate_level, identity })
    }

    pub fn path(&self) -> &Path { &self.path }
    pub fn identity(&self) -> &DeviceIdentity { &self.identity }
    pub fn set_identity(&mut self, identity: DeviceIdentity) { self.identity = identity; }
    pub fn write_gate_level(&self) -> WriteGateLevel { self.gate_level }
    pub fn set_write_gate_level(&mut self, level: WriteGateLevel) { self.gate_level = level; }

    pub fn read_state(&mut self) -> Result<DeviceState, PanteonError> { Transport::read_state(self) }
    pub fn read_register(&mut self, bank: u8, addr: u16) -> Result<u16, PanteonError> { Transport::read_register(self, bank, addr) }
    pub fn experimental_read_register(&mut self, bank: u8, addr: u16) -> Result<[u8; 7], PanteonError> { Transport::experimental_read_register(self, bank, addr) }
    pub fn experimental_read_byte(&mut self, addr: u16) -> Result<u8, PanteonError> { Transport::experimental_read_byte(self, addr) }
    pub fn experimental_read(&mut self, bank: u8, base: u16, count: usize, delay: Duration) -> Result<Vec<u8>, PanteonError> { Transport::experimental_read(self, bank, base, count, delay) }
    pub fn experimental_read_chunk(&mut self, base: u16, count: usize) -> Result<Vec<u8>, PanteonError> { self.experimental_read(0, base, count, Self::READ_PROTOCOL_DELAY) }
    pub fn experimental_read_bank(&mut self, bank: u8, addr: u16, count: usize) -> Result<Vec<u8>, PanteonError> { self.experimental_read(bank, addr, count, Self::READ_PROTOCOL_DELAY) }
    pub fn write_feature(&mut self, report: &[u8; FEATURE_REPORT_LEN]) -> Result<(), PanteonError> { Transport::write_feature(self, report) }
    pub fn execute_transaction(&mut self, plan: &WriteTransactionPlan, dry_run: bool) -> Result<(), PanteonError> { Transport::execute_transaction(self, plan, dry_run) }
    pub fn execute_transaction_with_settle(&mut self, plan: &WriteTransactionPlan, dry_run: bool, settle_ms: u64) -> Result<(), PanteonError> { Transport::execute_transaction_with_settle(self, plan, dry_run, settle_ms) }
    pub fn read_report_descriptor(&self) -> Result<Vec<u8>, PanteonError> { read::read_report_descriptor(&self.file) }
}

impl Transport for Device {
    fn device_identity(&self) -> DeviceIdentity { self.identity }
    fn write_gate_level(&self) -> WriteGateLevel { self.gate_level }
    fn set_write_gate_level(&mut self, level: WriteGateLevel) { self.gate_level = level; }

    fn read_feature(&mut self, report: &mut [u8; FEATURE_REPORT_LEN]) -> Result<(), PanteonError> {
        read::read_feature_ioctl(&self.file, report)
    }

    fn raw_write_feature(&mut self, report: &[u8; FEATURE_REPORT_LEN]) -> Result<(), PanteonError> {
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::io::AsRawFd;
            const HIDIOCSFEATURE_8: std::os::raw::c_ulong = 0xC0084806;
            let fd = self.file.as_raw_fd();
            extern "C" { fn ioctl(fd: std::os::raw::c_int, req: std::os::raw::c_ulong, ...) -> std::os::raw::c_int; }
            let mut buf = *report;
            let ret = unsafe { ioctl(fd, HIDIOCSFEATURE_8, buf.as_mut_ptr()) };
            if ret < 0 { return Err(PanteonError::Io(std::io::Error::last_os_error().to_string())); }
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = report;
            Err(PanteonError::Io("hidraw feature reports only supported on Linux".to_string()))
        }
    }
}
