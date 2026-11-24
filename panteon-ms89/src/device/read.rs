use std::time::Duration;

use crate::codec::{
    decode_descriptor_page, encode_read_request, encode_read_reset_frame, encode_read_sweep_frame,
    DescriptorPage, ReadPhase, ReadSequenceFrame,
};
use crate::model::{registers, DeviceState, StateConfidence};
use crate::{PanteonError, FEATURE_REPORT_LEN, REPORT_ID};
use super::gate::WriteGateLevel;
use super::Transport;

pub fn read_state<T: Transport + ?Sized>(transport: &mut T) -> Result<DeviceState, PanteonError> {
    let mut report = [0u8; FEATURE_REPORT_LEN];
    transport.read_feature(&mut report)?;
    let identity = transport.device_identity();
    let accepted = if identity.accepted_signatures.is_empty() {
        crate::model::ACCEPTED_SIGNATURES
    } else {
        identity.accepted_signatures
    };
    DeviceState::from_feature_report_with_signatures(&report, accepted)
}

pub fn read_register<T: Transport + ?Sized>(transport: &mut T, bank: u8, addr: u16) -> Result<u16, PanteonError> {
    let state = transport.read_state()?;
    if state.confidence == StateConfidence::RegistersConfirmed {
        if bank == registers::ACTIVE_DPI_SLOT.0 && addr == registers::ACTIVE_DPI_SLOT.1 {
            if let Some(val) = state.raw_slot_value { return Ok(val); }
        }
        if bank == registers::POLLING_RATE.0 && addr == registers::POLLING_RATE.1 {
            if let Some(val) = state.raw_polling_value { return Ok(val); }
        }
    }
    Err(PanteonError::Protocol("Arbitrary register read is not supported by device firmware without hardware write (blocked by Write-Gate)"))
}

pub fn experimental_read_register<T: Transport + ?Sized>(transport: &mut T, bank: u8, addr: u16) -> Result<[u8; 7], PanteonError> {
    let frame = encode_read_request(bank, addr);
    transport.write_feature(&frame)?;
    std::thread::sleep(Duration::from_millis(2));
    let mut report = [0u8; FEATURE_REPORT_LEN];
    report[0] = REPORT_ID;
    transport.read_feature(&mut report)?;
    if report[0] != REPORT_ID {
        return Err(PanteonError::Protocol("Invalid report ID in readback response"));
    }
    let mut payload = [0u8; 7];
    payload.copy_from_slice(&report[1..8]);
    Ok(payload)
}

pub fn experimental_read_byte<T: Transport + ?Sized>(transport: &mut T, addr: u16) -> Result<u8, PanteonError> {
    let frame = encode_read_sweep_frame(0, addr, 0);
    transport.write_feature(&frame)?;
    std::thread::sleep(Duration::from_millis(2));
    let mut report = [0u8; FEATURE_REPORT_LEN];
    report[0] = REPORT_ID;
    transport.read_feature(&mut report)?;
    if report[0] != REPORT_ID {
        return Err(PanteonError::Protocol("Invalid report ID in sweep readback response"));
    }
    Ok(report[1])
}

pub fn experimental_read<T: Transport + ?Sized>(
    transport: &mut T,
    bank: u8,
    base: u16,
    count: usize,
    delay: Duration,
) -> Result<Vec<u8>, PanteonError> {
    if transport.write_gate_level() < WriteGateLevel::Level1AddressingOnly {
        return Err(PanteonError::WriteDisabled);
    }
    if bank > 7 {
        return Err(PanteonError::ProtocolDetailed {
            context: "experimental_read",
            detail: format!("Bank out of range: {} must be in 0..=7", bank),
        });
    }
    if count == 0 || count > 256 {
        return Err(PanteonError::ProtocolDetailed {
            context: "experimental_read",
            detail: format!("read count {} must be in 1..=256", count),
        });
    }
    if (base as usize) + count > 0x10000 {
        return Err(PanteonError::ProtocolDetailed {
            context: "experimental_read",
            detail: format!("read address range (0x{:04x} + {}) exceeds 16-bit space", base, count),
        });
    }
    let count_minus_1 = (count - 1) as u8;

    for i in 0..count {
        let (b_or_idx, f_addr) = if bank == 0 { (i as u8, base) } else { (bank, base + i as u16) };
        let frame = ReadSequenceFrame::new(ReadPhase::Prep, b_or_idx, f_addr, count_minus_1).encode();
        transport.raw_write_feature(&frame)?;
        if !delay.is_zero() { std::thread::sleep(delay); }
    }
    if !delay.is_zero() { std::thread::sleep(delay); }

    let arm = ReadSequenceFrame::new(ReadPhase::ArmFinalize, bank, base, 0).encode();
    transport.raw_write_feature(&arm)?;

    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let (b_or_idx, f_addr) = if bank == 0 { (i as u8, base) } else { (bank, base + i as u16) };
        let frame = ReadSequenceFrame::new(ReadPhase::ReadRequest, b_or_idx, f_addr, count_minus_1).encode();
        transport.raw_write_feature(&frame)?;
        if !delay.is_zero() { std::thread::sleep(delay); }
        let mut report = [0u8; FEATURE_REPORT_LEN];
        report[0] = REPORT_ID;
        transport.read_feature(&mut report)?;
        if report[0] != REPORT_ID {
            return Err(PanteonError::Protocol("Invalid report ID in readback response"));
        }
        out.push(report[1]);
    }

    if !delay.is_zero() { std::thread::sleep(delay); }
    transport.raw_write_feature(&encode_read_reset_frame())?;
    Ok(out)
}

pub fn experimental_read_chunk<T: Transport + ?Sized>(transport: &mut T, base: u16, count: usize) -> Result<Vec<u8>, PanteonError> {
    experimental_read(transport, 0, base, count, Duration::from_millis(2))
}

pub fn experimental_read_bank<T: Transport + ?Sized>(transport: &mut T, bank: u8, addr: u16, count: usize) -> Result<Vec<u8>, PanteonError> {
    experimental_read(transport, bank, addr, count, Duration::from_millis(2))
}

pub fn read_descriptor_page<T: Transport + ?Sized>(transport: &mut T) -> Result<DescriptorPage, PanteonError> {
    let bytes = experimental_read(transport, 0, 0, 32, Duration::from_millis(2))?;
    decode_descriptor_page(&bytes)
}

#[cfg(target_os = "linux")]
pub fn read_feature_ioctl(file: &std::fs::File, report: &mut [u8; FEATURE_REPORT_LEN]) -> Result<(), PanteonError> {
    use std::os::unix::io::AsRawFd;
    const HIDIOCGFEATURE_8: std::os::raw::c_ulong = 0xC0084807;
    report[0] = REPORT_ID;
    let fd = file.as_raw_fd();
    extern "C" { fn ioctl(fd: std::os::raw::c_int, req: std::os::raw::c_ulong, ...) -> std::os::raw::c_int; }
    let ret = unsafe { ioctl(fd, HIDIOCGFEATURE_8, report.as_mut_ptr()) };
    if ret < 0 { return Err(PanteonError::Io(std::io::Error::last_os_error().to_string())); }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn read_feature_ioctl(_file: &std::fs::File, _report: &mut [u8; FEATURE_REPORT_LEN]) -> Result<(), PanteonError> {
    Err(PanteonError::Io("hidraw only supported on Linux".to_string()))
}

#[cfg(target_os = "linux")]
pub fn read_report_descriptor(file: &std::fs::File) -> Result<Vec<u8>, PanteonError> {
    use std::os::unix::io::AsRawFd;
    const HIDIOCGRDESCSIZE: std::os::raw::c_ulong = 0x80044801;
    const HIDIOCGRDESC: std::os::raw::c_ulong = 0x90044802;
    let fd = file.as_raw_fd();
    extern "C" { fn ioctl(fd: std::os::raw::c_int, req: std::os::raw::c_ulong, ...) -> std::os::raw::c_int; }

    let mut size: u32 = 0;
    let ret = unsafe { ioctl(fd, HIDIOCGRDESCSIZE, &mut size as *mut u32) };
    if ret < 0 || size == 0 || size > 4096 {
        return Err(PanteonError::Io(std::io::Error::last_os_error().to_string()));
    }

    #[repr(C)]
    struct HidrawReportDescriptor { size: u32, value: [u8; 4096] }
    let mut rdesc = HidrawReportDescriptor { size, value: [0u8; 4096] };
    let ret = unsafe { ioctl(fd, HIDIOCGRDESC, &mut rdesc as *mut _) };
    if ret < 0 { return Err(PanteonError::Io(std::io::Error::last_os_error().to_string())); }
    Ok(rdesc.value[..(rdesc.size as usize).min(4096)].to_vec())
}

#[cfg(not(target_os = "linux"))]
pub fn read_report_descriptor(_file: &std::fs::File) -> Result<Vec<u8>, PanteonError> {
    Err(PanteonError::Io("hidraw only supported on Linux".to_string()))
}
