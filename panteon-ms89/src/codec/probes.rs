use crate::{PanteonError, FEATURE_REPORT_LEN, REPORT_ID};

/// Feature report commands for PANTEON MS89
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    MacroDelay = 0x0b,   // Hypothesis: 07 0b <val1_hi> <val1_lo> <val2_hi> <val2_lo> 00 00
    Probe = 0x10,        // Confirmed (GET handshake)
    Get = 0x11,          // Confirmed
    PageBase = 0x13,     // Hypothesis: 07 13 00 80 00 40 00 00 (страница/база)
    ProfileBlock = 0x14, // Hypothesis: содержит profile/bank, флаг, threshold
    Unknown15 = 0x15,    // Hypothesis, shape неизвестна — только variants enum
    Unknown16 = 0x16,    // Hypothesis, shape неизвестна — только variants enum
    Value16 = 0x17,      // Confirmed: 07 17 H L 00 00 00 00, H:L — big-endian u16
    PackedMem = 0x18,    // Confirmed: 07 18 <mode> <bank> <addr_lo> <addr_hi> <data_lo> <data_hi> 00
    Unknown20 = 0x20,    // Hypothesis, shape неизвестна — только variants enum
}

impl Command {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(val: u8) -> Result<Self, PanteonError> {
        match val {
            0x0b => Ok(Self::MacroDelay),
            0x10 => Ok(Self::Probe),
            0x11 => Ok(Self::Get),
            0x13 => Ok(Self::PageBase),
            0x14 => Ok(Self::ProfileBlock),
            0x15 => Ok(Self::Unknown15),
            0x16 => Ok(Self::Unknown16),
            0x17 => Ok(Self::Value16),
            0x18 => Ok(Self::PackedMem),
            0x20 => Ok(Self::Unknown20),
            _ => Err(PanteonError::Protocol("Unknown command byte")),
        }
    }
}

/// Encode 0x10 Probe command feature report (GET handshake)
pub fn encode_probe() -> [u8; FEATURE_REPORT_LEN] {
    [REPORT_ID, Command::Probe as u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
}

/// Encode 0x11 Get command feature report
pub fn encode_get() -> [u8; FEATURE_REPORT_LEN] {
    [REPORT_ID, Command::Get as u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
}

/// Candidate page/base probe vector "07 13 00 80 00 40 00 00".
/// UNTESTED: identified during protocol probing (live USB captures).
pub const HYPOTHETICAL_PAGE_BASE_PROBE: [u8; FEATURE_REPORT_LEN] = [
    REPORT_ID, 0x13, 0x00, 0x80, 0x00, 0x40, 0x00, 0x00,
];

/// Encode 0x13 PageBase command feature report (Hypothesis: 07 13 00 80 00 40 00 00)
pub fn encode_page_base() -> [u8; FEATURE_REPORT_LEN] {
    HYPOTHETICAL_PAGE_BASE_PROBE
}

/// UNTESTED: Encode candidate page/base memory addressing request (command 0x13).
pub fn encode_read_request_0x13(page: u16, base: u16) -> [u8; FEATURE_REPORT_LEN] {
    let [p_hi, p_lo] = page.to_be_bytes();
    let [b_hi, b_lo] = base.to_be_bytes();
    [
        REPORT_ID,
        Command::PageBase as u8, // 0x13
        p_hi,
        p_lo,
        b_hi,
        b_lo,
        0x00,
        0x00,
    ]
}

/// Encode 0x14 ProfileBlock command feature report
/// Hypothesis: contains profile/bank, flag, threshold
pub fn encode_profile_block(profile_bank: u8, flag: u8, threshold: u16) -> [u8; FEATURE_REPORT_LEN] {
    let [th_h, th_l] = threshold.to_be_bytes();
    [REPORT_ID, Command::ProfileBlock as u8, profile_bank, flag, th_h, th_l, 0x00, 0x00]
}

/// Decode 0x14 ProfileBlock command feature report
pub fn decode_profile_block(report: &[u8]) -> Result<(u8, u8, u16), PanteonError> {
    if report.len() < 6 {
        return Err(PanteonError::Protocol("ProfileBlock report too short"));
    }
    if report[0] != REPORT_ID {
        return Err(PanteonError::Protocol("Invalid report ID"));
    }
    if report[1] != Command::ProfileBlock as u8 {
        return Err(PanteonError::Protocol("Invalid command byte for ProfileBlock"));
    }
    let profile_bank = report[2];
    let flag = report[3];
    let threshold = u16::from_be_bytes([report[4], report[5]]);
    Ok((profile_bank, flag, threshold))
}

/// Encode 0x17 Value16 command feature report (Confirmed: 07 17 H L 00 00 00 00)
pub fn encode_value16(val: u16) -> [u8; FEATURE_REPORT_LEN] {
    let [h, l] = val.to_be_bytes();
    [REPORT_ID, Command::Value16 as u8, h, l, 0x00, 0x00, 0x00, 0x00]
}

/// Alias for encode_value16
pub fn encode_0x17(val: u16) -> [u8; FEATURE_REPORT_LEN] {
    encode_value16(val)
}

/// Decode 0x17 Value16 command feature report
pub fn decode_value16(report: &[u8]) -> Result<u16, PanteonError> {
    if report.len() < 4 {
        return Err(PanteonError::Protocol("Value16 report too short"));
    }
    if report[0] != REPORT_ID {
        return Err(PanteonError::Protocol("Invalid report ID"));
    }
    if report[1] != Command::Value16 as u8 {
        return Err(PanteonError::Protocol("Invalid command byte for Value16"));
    }
    Ok(u16::from_be_bytes([report[2], report[3]]))
}

/// Encode an empty command report with specified command byte
pub fn encode_command(cmd: Command) -> [u8; FEATURE_REPORT_LEN] {
    [REPORT_ID, cmd as u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
}

/// Decode command from report
pub fn decode_command(report: &[u8]) -> Result<Command, PanteonError> {
    if report.len() < 2 {
        return Err(PanteonError::Protocol("Report too short"));
    }
    if report[0] != REPORT_ID {
        return Err(PanteonError::Protocol("Invalid report ID"));
    }
    Command::from_u8(report[1])
}

/// Encode 0x0b MacroDelay command feature report (Hypothesis: 07 0b H1 L1 H2 L2 00 00)
pub fn encode_macro_delay(val1: u16, val2: u16) -> [u8; FEATURE_REPORT_LEN] {
    let [v1_h, v1_l] = val1.to_be_bytes();
    let [v2_h, v2_l] = val2.to_be_bytes();
    [REPORT_ID, Command::MacroDelay as u8, v1_h, v1_l, v2_h, v2_l, 0x00, 0x00]
}

/// Encode 0x15 Macro sequence candidate command (Hypothesis: 07 15 D0 D1 D2 D3 D4 D5)
pub fn encode_0x15(args: [u8; 6]) -> [u8; FEATURE_REPORT_LEN] {
    [REPORT_ID, Command::Unknown15 as u8, args[0], args[1], args[2], args[3], args[4], args[5]]
}

/// Encode 0x16 Macro modifier flags candidate command (Hypothesis: 07 16 00 Flag 00 00 00 00)
pub fn encode_0x16(flag: u8) -> [u8; FEATURE_REPORT_LEN] {
    [REPORT_ID, Command::Unknown16 as u8, 0x00, flag, 0x00, 0x00, 0x00, 0x00]
}

/// Encode 0x20 Profile apply/resync candidate command (Hypothesis: 07 20 00 00 00 00 00 00)
pub fn encode_0x20() -> [u8; FEATURE_REPORT_LEN] {
    [REPORT_ID, Command::Unknown20 as u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
}

/// Encode 0x18 PackedMem Mode 0x10 command (Flash persist / flush)
/// Wire format: 07 18 10 00 00 00 00 00
pub fn encode_packed_mem_mode_0x10() -> [u8; FEATURE_REPORT_LEN] {
    [REPORT_ID, Command::PackedMem as u8, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00]
}

/// Encode generic command probe with raw arguments (up to 6 bytes).
pub fn encode_probe_command(cmd: u8, args: &[u8]) -> Result<[u8; FEATURE_REPORT_LEN], PanteonError> {
    if args.len() > 6 {
        return Err(PanteonError::ProtocolDetailed {
            context: "encode_probe_command",
            detail: format!("too many args ({}), maximum is 6", args.len()),
        });
    }
    let mut frame = [0u8; FEATURE_REPORT_LEN];
    frame[0] = REPORT_ID;
    frame[1] = cmd;
    frame[2..2 + args.len()].copy_from_slice(args);
    Ok(frame)
}
