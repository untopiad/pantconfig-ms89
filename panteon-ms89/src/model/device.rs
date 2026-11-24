use crate::codec::{Command, DeviceGetReportResponse};
use crate::model::profile::PollingRate;
use crate::model::registers;
use crate::{PanteonError, FEATURE_REPORT_LEN, REPORT_ID};

pub const ACCEPTED_SIGNATURES: &[u16] = &[0x0976, 0x0978];

/// Target hardware identity definition for PANTEON MS89 USB gaming mouse
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceIdentity {
    pub pid: u16,
    pub protocol_verified: bool,  // write-протокол 0x18 проверен живьём на этом PID
    pub accepted_signatures: &'static [u16], // ожидаемые сигнатуры GET_REPORT
}

impl DeviceIdentity {
    pub const fn unknown() -> Self {
        Self {
            pid: 0,
            protocol_verified: false,
            accepted_signatures: &[],
        }
    }

    pub fn find(pid: u16) -> Option<&'static DeviceIdentity> {
        KNOWN_IDENTITIES.iter().find(|id| id.pid == pid)
    }

    pub fn is_known(pid: u16) -> bool {
        Self::find(pid).is_some()
    }

    pub fn accepts_signature(&self, sig: u16) -> bool {
        self.accepted_signatures.contains(&sig)
    }
}

pub const KNOWN_IDENTITIES: &[DeviceIdentity] = &[
    DeviceIdentity { pid: 0x1440, protocol_verified: true,  accepted_signatures: ACCEPTED_SIGNATURES },
    DeviceIdentity { pid: 0x1e01, protocol_verified: false, accepted_signatures: ACCEPTED_SIGNATURES },
];

/// Operation mode for 0x18 PackedMem command confirmed by live USB traces
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PackedMemMode {
    FinalCommit = 0x00,
    Write = 0x03,
    CommitBank = 0x09,
}

impl PackedMemMode {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for PackedMemMode {
    type Error = PanteonError;

    fn try_from(val: u8) -> Result<Self, Self::Error> {
        match val {
            0x00 => Ok(Self::FinalCommit),
            0x03 => Ok(Self::Write),
            0x09 => Ok(Self::CommitBank),
            _ => Err(PanteonError::Protocol("Invalid PackedMemMode byte")),
        }
    }
}

impl From<PackedMemMode> for u8 {
    fn from(mode: PackedMemMode) -> Self {
        mode as u8
    }
}

/// 16-bit packed memory layout for command 0x18 confirmed by live USB traces:
/// Wire layout (9 bytes total feature report buffer):
/// `[0x07, 0x18, mode, bank, addr_lo, addr_hi, data_lo, data_hi, 0x00]`
///
/// Note on byte 8 (index 8):
/// In live USB captures (wLength=8), only 8 bytes are transmitted on the wire (b0..b7).
/// The apparent trailing `0x07` in raw dumps (e.g. `07 18 03 06 18 00 01 07`) is
/// actually `data_hi` (0x0701), not a pad byte. In the 9-byte HID feature report buffer,
/// byte 8 is always 0x00 (padding).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackedMemLayout {
    pub mode: u8,
    pub bank: u8,
    pub addr: u16,
    pub data: u16,
}

impl PackedMemLayout {
    pub fn new(mode: u8, bank: u8, addr: u16, data: u16) -> Self {
        Self {
            mode,
            bank,
            addr,
            data,
        }
    }

    /// Pack layout into feature report
    pub fn pack(&self) -> [u8; FEATURE_REPORT_LEN] {
        let [addr_lo, addr_hi] = self.addr.to_le_bytes();
        let [data_lo, data_hi] = self.data.to_le_bytes();
        [
            REPORT_ID,
            Command::PackedMem as u8,
            self.mode,
            self.bank,
            addr_lo,
            addr_hi,
            data_lo,
            data_hi,
        ]
    }

    /// Unpack feature report into PackedMemLayout with strict validation
    pub fn unpack(report: &[u8]) -> Result<Self, PanteonError> {
        if report.len() < 8 {
            return Err(PanteonError::Protocol("PackedMem report too short"));
        }
        if report[0] != REPORT_ID {
            return Err(PanteonError::Protocol("Invalid report ID"));
        }
        if report[1] != Command::PackedMem as u8 {
            return Err(PanteonError::Protocol("Invalid command byte for PackedMem"));
        }
        let mode = report[2];
        let _ = PackedMemMode::try_from(mode)?;

        let bank = report[3];
        let addr = u16::from_le_bytes([report[4], report[5]]);
        let data = u16::from_le_bytes([report[6], report[7]]);

        Ok(Self {
            mode,
            bank,
            addr,
            data,
        })
    }

    /// Parse mode into PackedMemMode enum
    pub fn parsed_mode(&self) -> Result<PackedMemMode, PanteonError> {
        PackedMemMode::try_from(self.mode)
    }
}

/// Level of confidence in the decoded device state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateConfidence {
    /// Confirmed via live/trace register memory map reads
    RegistersConfirmed,
    /// Only handshake frame received; status/flags register semantics unconfirmed
    HandshakeOnly,
}

/// Live state of the PANTEON MS89 device
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceState {
    /// Active DPI slot (1-based: 1 through 6), if confirmed
    pub active_dpi_slot: Option<u8>,
    /// Raw active DPI slot register value (e.g. 0x0718 for slot 1), if confirmed
    pub raw_slot_value: Option<u16>,
    /// Polling rate (125, 250, 500, or 1000 Hz), if confirmed
    pub polling_rate: Option<PollingRate>,
    /// Raw polling rate register value (e.g. 0x0700 for 1000 Hz), if confirmed
    pub raw_polling_value: Option<u16>,
    /// Optional raw handshake response from device (GET_REPORT)
    pub handshake: Option<DeviceGetReportResponse>,
    /// State decoding confidence level
    pub confidence: StateConfidence,
}

impl DeviceState {
    /// Create new confirmed DeviceState with validated slot (1..=6) and polling rate
    pub fn new(active_dpi_slot: u8, polling_rate: PollingRate) -> Result<Self, PanteonError> {
        let raw_slot_value = Self::slot_index_to_raw(active_dpi_slot)?;
        let raw_polling_value = polling_rate.to_register_code();
        Ok(Self {
            active_dpi_slot: Some(active_dpi_slot),
            raw_slot_value: Some(raw_slot_value),
            polling_rate: Some(polling_rate),
            raw_polling_value: Some(raw_polling_value),
            handshake: None,
            confidence: StateConfidence::RegistersConfirmed,
        })
    }

    /// Convert 1-based slot index (1..=6) to raw register value: 0x0700 | (0x18 + slot - 1)
    pub fn slot_index_to_raw(slot: u8) -> Result<u16, PanteonError> {
        if !(1..=6).contains(&slot) {
            return Err(PanteonError::Protocol("DPI slot index out of range (must be 1..=6)"));
        }
        Ok(0x0700 | (0x18 + (slot - 1) as u16))
    }

    /// Convert raw register value (0x0718..=0x071d) to 1-based slot index (1..=6)
    pub fn raw_to_slot_index(raw: u16) -> Result<u8, PanteonError> {
        let low = (raw & 0xFF) as u8;
        if raw & 0xFF00 == 0x0700 && (0x18..=0x1D).contains(&low) {
            Ok(low - 0x18 + 1)
        } else {
            Err(PanteonError::Protocol("Invalid raw DPI slot register value"))
        }
    }

    /// Construct DeviceState from a list of register entries `(bank, addr, data)`.
    /// Searches strictly for the confirmed registers:
    /// - `ACTIVE_DPI_SLOT`: (Bank 0, Addr 0x0018)
    /// - `POLLING_RATE`: (Bank 6, Addr 0x0018)
    pub fn from_registers(entries: &[(u8, u16, u16)]) -> Result<Self, PanteonError> {
        let mut raw_slot = None;
        let mut raw_polling = None;

        for &(bank, addr, data) in entries {
            if (bank, addr) == registers::ACTIVE_DPI_SLOT {
                raw_slot = Some(data);
            } else if (bank, addr) == registers::POLLING_RATE {
                raw_polling = Some(data);
            }
        }

        let raw_slot_val = raw_slot.ok_or(PanteonError::Protocol(
            "Missing ACTIVE_DPI_SLOT (Bank 0, Addr 0x0018) register in state read",
        ))?;
        let raw_polling_val = raw_polling.ok_or(PanteonError::Protocol(
            "Missing POLLING_RATE (Bank 6, Addr 0x0018) register in state read",
        ))?;

        let active_dpi_slot = Self::raw_to_slot_index(raw_slot_val)?;
        let polling_rate = PollingRate::from_register_code(raw_polling_val).ok_or(
            PanteonError::Protocol("Invalid or unknown polling rate register value"),
        )?;

        Ok(Self {
            active_dpi_slot: Some(active_dpi_slot),
            raw_slot_value: Some(raw_slot_val),
            polling_rate: Some(polling_rate),
            raw_polling_value: Some(raw_polling_val),
            handshake: None,
            confidence: StateConfidence::RegistersConfirmed,
        })
    }

    /// Construct DeviceState from a slice of PackedMem feature reports
    pub fn from_packed_reports<T: AsRef<[u8]>>(reports: &[T]) -> Result<Self, PanteonError> {
        let mut entries = Vec::with_capacity(reports.len());
        for report in reports {
            let layout = PackedMemLayout::unpack(report.as_ref())?;
            entries.push((layout.bank, layout.addr, layout.data));
        }
        Self::from_registers(&entries)
    }

    /// Construct DeviceState from a confirmed live GET_REPORT response frame.
    ///
    /// NOTE: handshake format confirmed (07 00 76 09 <rev> 00 <st> <fl>), семантика
    /// status/flags НЕ подтверждена: опровергнуто перекрёстной сверкой с memmap
    /// (ctrl_h..m: одинаковый handshake при разных слотах 2..6; ctrl_m: 125 Гц при flags=0x5a).
    pub fn from_get_report_response(resp: &DeviceGetReportResponse) -> Result<Self, PanteonError> {
        Ok(Self {
            active_dpi_slot: None,
            raw_slot_value: None,
            polling_rate: None,
            raw_polling_value: None,
            handshake: Some(*resp),
            confidence: StateConfidence::HandshakeOnly,
        })
    }

    /// Construct DeviceState from any confirmed feature report buffer expecting accepted handshake signatures
    pub fn from_feature_report_with_signatures(report: &[u8], accepted_sigs: &[u16]) -> Result<Self, PanteonError> {
        if report.len() < 8 {
            return Err(PanteonError::ProtocolDetailed {
                context: "from_feature_report",
                detail: format!(
                    "Feature report too short ({} < 8): raw=[{}]",
                    report.len(),
                    crate::codec::handshake::hex_format(report)
                ),
            });
        }
        if report[0] != REPORT_ID {
            return Err(PanteonError::ProtocolDetailed {
                context: "from_feature_report",
                detail: format!(
                    "Invalid report ID 0x{:02x} (expected 0x{:02x}): raw=[{}]",
                    report[0],
                    REPORT_ID,
                    crate::codec::handshake::hex_format(report)
                ),
            });
        }
        if report[1] == Command::PackedMem as u8 {
            let layout = PackedMemLayout::unpack(report)?;
            Self::from_packed_reports(&[layout.pack()])
        } else {
            let resp = DeviceGetReportResponse::decode_with_expected_signatures(report, accepted_sigs)?;
            Self::from_get_report_response(&resp)
        }
    }

    /// Construct DeviceState from any confirmed feature report buffer expecting a specific handshake signature
    pub fn from_feature_report_with_signature(report: &[u8], expected_sig: u16) -> Result<Self, PanteonError> {
        Self::from_feature_report_with_signatures(report, &[expected_sig])
    }

    /// Construct DeviceState from any confirmed feature report buffer (accepts known canonical signatures)
    pub fn from_feature_report(report: &[u8]) -> Result<Self, PanteonError> {
        Self::from_feature_report_with_signatures(report, ACCEPTED_SIGNATURES)
    }
}
