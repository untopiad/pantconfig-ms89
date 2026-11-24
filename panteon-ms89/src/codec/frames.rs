use crate::model::{PackedMemLayout, PackedMemMode};
use crate::{PanteonError, FEATURE_REPORT_LEN};

/// Encode 0x18 PackedMem command feature report:
/// `[0x07, 0x18, mode, bank, addr_lo, addr_hi, data_lo, data_hi]`
pub fn encode_packed_mem(layout: &PackedMemLayout) -> [u8; FEATURE_REPORT_LEN] {
    layout.pack()
}

/// Decode 0x18 PackedMem command feature report
pub fn decode_packed_mem(report: &[u8]) -> Result<PackedMemLayout, PanteonError> {
    PackedMemLayout::unpack(report)
}

/// Mode 0x08: REFUTED. Tested on live hardware across multiple addresses (Bank 0, 5, 6 0x0018);
/// yielded identical static handshake responses regardless of target register address.
pub const READ_REQUEST_MODE_08_REFUTED: u8 = 0x08;

/// Mode 0x05: linear read request mode derived from protocol probing (live USB captures, completely recalculated).
/// Wire frame (8 bytes): `07 18 05 <i> <addr_lo> <addr_hi> 00 <count_minus_1>`
pub const READ_REQUEST_MODE: u8 = 0x05;

/// Read sequence phase for two-phase memory read protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadPhase {
    Prep,
    ArmFinalize,
    ReadRequest,
}

/// Unified parameterized frame builder for 0x18 PackedMem read sequences
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadSequenceFrame {
    pub phase: ReadPhase,
    pub bank: u8,
    pub addr: u16,
    pub count_minus_1: u8,
}

impl ReadSequenceFrame {
    pub fn new(phase: ReadPhase, bank: u8, addr: u16, count_minus_1: u8) -> Self {
        Self {
            phase,
            bank,
            addr,
            count_minus_1,
        }
    }

    /// Assembles the 8-byte HID feature report for this read frame
    pub fn encode(&self) -> [u8; FEATURE_REPORT_LEN] {
        let mode = match self.phase {
            ReadPhase::Prep => PackedMemMode::CommitBank as u8,          // 0x09
            ReadPhase::ArmFinalize => PackedMemMode::FinalCommit as u8, // 0x00
            ReadPhase::ReadRequest => READ_REQUEST_MODE,                 // 0x05
        };
        let data_hi = match self.phase {
            ReadPhase::ArmFinalize => 0x00,
            ReadPhase::Prep | ReadPhase::ReadRequest => self.count_minus_1,
        };
        let data = u16::from_le_bytes([0x00, data_hi]);
        PackedMemLayout::new(mode, self.bank, self.addr, data).pack()
    }

    pub fn to_report(&self) -> [u8; FEATURE_REPORT_LEN] {
        self.encode()
    }
}

impl From<ReadSequenceFrame> for [u8; FEATURE_REPORT_LEN] {
    fn from(frame: ReadSequenceFrame) -> Self {
        frame.encode()
    }
}

/// Encode linear read sweep frame (mode 0x05, protocol-probed, live capture):
/// `07 18 05 <i> <addr_lo> <addr_hi> 00 <count_minus_1>` (8 bytes)
/// - `index`: index within chunk (i)
/// - `addr`: linear base address of chunk
/// - `count_minus_1`: count of bytes minus 1 (0 for single byte chunk)
pub fn encode_read_sweep_frame(index: u8, addr: u16, count_minus_1: u8) -> [u8; FEATURE_REPORT_LEN] {
    ReadSequenceFrame::new(ReadPhase::ReadRequest, index, addr, count_minus_1).encode()
}

/// READ-context PREP frame (protocol-verified, live capture): `07 18 09 <i> <addr_lo> <addr_hi> 00 <count_minus_1>`.
/// In WRITE context mode 0x09 = commit bank; in READ context it STAGES one byte slot of the
/// pending read range. The device requires the full prep phase (count frames) before the
/// read loop returns memory data instead of the status echo.
pub fn encode_read_prep_frame(index: u8, addr: u16, count_minus_1: u8) -> [u8; FEATURE_REPORT_LEN] {
    ReadSequenceFrame::new(ReadPhase::Prep, index, addr, count_minus_1).encode()
}

/// READ-context bank-specific PREP frame: `07 18 09 <bank> <addr_lo> <addr_hi> 00 <count_minus_1>`.
pub fn encode_read_bank_prep_frame(bank: u8, addr: u16, count_minus_1: u8) -> [u8; FEATURE_REPORT_LEN] {
    ReadSequenceFrame::new(ReadPhase::Prep, bank, addr, count_minus_1).encode()
}

/// READ-context bank-specific arm-finalize frame: `07 18 00 <bank> <addr_lo> <addr_hi> 00 00`.
pub fn encode_read_bank_arm_finalize(bank: u8, addr: u16) -> [u8; FEATURE_REPORT_LEN] {
    ReadSequenceFrame::new(ReadPhase::ArmFinalize, bank, addr, 0).encode()
}

/// READ-context bank-specific read request frame: `07 18 05 <bank> <addr_lo> <addr_hi> 00 <count_minus_1>`.
pub fn encode_read_bank_frame(bank: u8, addr: u16, count_minus_1: u8) -> [u8; FEATURE_REPORT_LEN] {
    ReadSequenceFrame::new(ReadPhase::ReadRequest, bank, addr, count_minus_1).encode()
}

/// READ-context arm-complete frame (protocol-verified, live capture tail): `07 18 00 00 <addr_lo> <addr_hi> 00 00`.
/// mode 0x00 with the base address closes the prep/staging phase before the read loop starts.
pub fn encode_read_arm_finalize(addr: u16) -> [u8; FEATURE_REPORT_LEN] {
    ReadSequenceFrame::new(ReadPhase::ArmFinalize, 0x00, addr, 0).encode()
}

/// READ-context post-loop reset frame (protocol-verified, live capture tail): `07 18 00 00 00 00 00 00`.
/// Builder called with all-zero args after the read loop completes.
pub fn encode_read_reset_frame() -> [u8; FEATURE_REPORT_LEN] {
    ReadSequenceFrame::new(ReadPhase::ArmFinalize, 0x00, 0x0000, 0).encode()
}

/// UNTESTED: Encode candidate read request using custom mode in 0x18 PackedMem frame.
pub fn encode_read_request_mode(mode: u8, bank: u8, addr: u16) -> [u8; FEATURE_REPORT_LEN] {
    PackedMemLayout::new(mode, bank, addr, 0x0000).pack()
}

/// Encode read request using mode 0x05 (protocol-probed, live capture):
/// `07 18 05 <bank> <addr_lo> <addr_hi> 00 00` (8 bytes)
pub fn encode_read_request(bank: u8, addr: u16) -> [u8; FEATURE_REPORT_LEN] {
    ReadSequenceFrame::new(ReadPhase::ReadRequest, bank, addr, 0).encode()
}

/// Encode read completion / reset frame: 07 18 05 00 00 00 00 00
pub fn encode_read_reset() -> [u8; FEATURE_REPORT_LEN] {
    ReadSequenceFrame::new(ReadPhase::ReadRequest, 0x00, 0x0000, 0).encode()
}

/// Confirmed preamble config unlock register: (Bank 0, Addr 0x0001, Data 0x0003).
/// Verified across all 13 USBPcap traces as the mandatory opening sequence before configuration writes;
/// without it the microcontroller returns STALL (os error 71 / EPROTO).
pub const CONFIG_UNLOCK: (u8, u16, u16) = (0x00, 0x0001, 0x0003);

/// Encode preamble data frame (P1): 07 18 03 00 01 00 03 00
pub fn encode_config_unlock() -> [u8; FEATURE_REPORT_LEN] {
    PackedMemLayout::new(
        PackedMemMode::Write as u8,
        CONFIG_UNLOCK.0,
        CONFIG_UNLOCK.1,
        CONFIG_UNLOCK.2,
    )
    .pack()
}

/// Encode preamble commit bank frame (P2): 07 18 09 00 01 00 00 00
pub fn encode_config_unlock_commit_bank() -> [u8; FEATURE_REPORT_LEN] {
    PackedMemLayout::new(
        PackedMemMode::CommitBank as u8,
        0x00,
        CONFIG_UNLOCK.1,
        0x0000,
    )
    .pack()
}

/// Encode preamble final commit frame (P3): 07 18 00 00 01 00 00 00
pub fn encode_config_unlock_final_commit() -> [u8; FEATURE_REPORT_LEN] {
    PackedMemLayout::new(
        PackedMemMode::FinalCommit as u8,
        0x00,
        CONFIG_UNLOCK.1,
        0x0000,
    )
    .pack()
}

/// Encodes the 5-frame Soft-Apply commit tail confirmed by live Windows driver traces (867-frame Apply sequence).
///
/// Traces show that every driver write session completes with this mandatory tail:
/// 1. Bank 0, Addr 0x007f Data Write (0x0000):  `07 18 03 00 7f 00 00 00`
/// 2. Bank 0, Addr 0x007f Commit Bank:         `07 18 09 00 7f 00 00 00`
/// 3. Bank 0, Addr 0x007f Final Commit:        `07 18 00 00 7f 00 00 00`
/// 4. Flash Flush command (mode 0x10):          `07 18 10 00 00 00 00 00`
/// 5. Post-flush idle frame (mode 0x00):        `07 18 00 00 00 00 00 00`
pub fn encode_soft_apply_tail() -> Vec<[u8; FEATURE_REPORT_LEN]> {
    vec![
        [0x07, 0x18, 0x03, 0x00, 0x7f, 0x00, 0x00, 0x00],
        [0x07, 0x18, 0x09, 0x00, 0x7f, 0x00, 0x00, 0x00],
        [0x07, 0x18, 0x00, 0x00, 0x7f, 0x00, 0x00, 0x00],
        [0x07, 0x18, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00],
        [0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    ]
}

/// Encodes Soft-Apply commit tail, optionally including or omitting the Flash Flush frame (0x10).
/// When `flash_flush` is false, frames 1..3 and 5 are returned (flash wear endurance protection).
pub fn encode_soft_apply_tail_frames(flash_flush: bool) -> Vec<[u8; FEATURE_REPORT_LEN]> {
    let full = encode_soft_apply_tail();
    if flash_flush {
        full
    } else {
        full.into_iter().filter(|f| f[2] != 0x10).collect()
    }
}

