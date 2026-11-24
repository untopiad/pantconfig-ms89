use crate::{PanteonError, FEATURE_REPORT_LEN, REPORT_ID};

pub fn hex_format(buf: &[u8]) -> String {
    buf.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
}

/// Confirmed GET_REPORT Feature Report response format from live USB traces:
/// Wire layout: `[0x07, <state_byte>, <sig_lo>, <sig_hi>, revision, 0x00, status, flags, 0x00]`
///
/// Note on byte 1 (state byte):
/// byte 1 — волатильный status-байт прошивки (наблюдения: 0x00 fresh, 0xdf post-probe,
/// 0xff post-write); семантика unresolved (confidence-gating), идентичность = сигнатура.
///
/// Note on difference from PackedMem (0x18):
/// In live USB captures, GET_REPORT returns an 8-byte status/handshake frame starting with Report ID 0x07,
/// volatile status byte 1, followed by hardware signature (0x0976 or 0x0978 LE),
/// dynamic revision byte, status byte (0x23 / 0x03), and state flags (0x7a / 0x5a / 0x0a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceGetReportResponse {
    pub report_id: u8,
    pub command: u8,
    pub signature: u16,
    pub revision: u8,
    pub status: u8,
    pub flags: u8,
    pub state_byte: u8,
}

impl DeviceGetReportResponse {
    /// Decode confirmed GET_REPORT response accepting any of the accepted device signatures.
    /// Byte 1 is treated as volatile firmware state byte (any value accepted).
    pub fn decode_with_expected_signatures(buf: &[u8], accepted_sigs: &[u16]) -> Result<Self, PanteonError> {
        if buf.len() < 8 {
            return Err(PanteonError::ProtocolDetailed {
                context: "decode_handshake",
                detail: format!(
                    "Truncated GET_REPORT response (expected at least 8 bytes, got {}): raw=[{}]",
                    buf.len(),
                    hex_format(buf)
                ),
            });
        }
        if buf[0] != REPORT_ID {
            return Err(PanteonError::ProtocolDetailed {
                context: "decode_handshake",
                detail: format!(
                    "Invalid report ID 0x{:02x} in GET_REPORT response (expected 0x{:02x}): raw=[{}]",
                    buf[0],
                    REPORT_ID,
                    hex_format(buf)
                ),
            });
        }

        let state_byte = buf[1];
        let signature = u16::from_le_bytes([buf[2], buf[3]]);
        if !accepted_sigs.is_empty() && !accepted_sigs.contains(&signature) {
            let accepted_str = accepted_sigs
                .iter()
                .map(|s| format!("0x{:04x}", s))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(PanteonError::ProtocolDetailed {
                context: "decode_handshake",
                detail: format!(
                    "Invalid device signature 0x{:04x} in GET_REPORT response (accepted: [{}], state: 0x{:02x}): raw=[{}]",
                    signature,
                    accepted_str,
                    state_byte,
                    hex_format(buf)
                ),
            });
        }

        let revision = buf[4];
        let status = buf[6];
        let flags = buf[7];
        Ok(Self {
            report_id: buf[0],
            command: state_byte,
            signature,
            revision,
            status,
            flags,
            state_byte,
        })
    }

    /// Decode confirmed GET_REPORT response expecting a specific device signature.
    pub fn decode_with_expected_signature(buf: &[u8], expected_sig: u16) -> Result<Self, PanteonError> {
        Self::decode_with_expected_signatures(buf, &[expected_sig])
    }

    /// Decode confirmed GET_REPORT response from arbitrary slice (accepts known canonical signatures).
    pub fn decode_slice(buf: &[u8]) -> Result<Self, PanteonError> {
        Self::decode_with_expected_signatures(buf, &[0x0976, 0x0978])
    }

    /// Decode confirmed feature GET_REPORT response from live traces.
    pub fn decode(report: &[u8; FEATURE_REPORT_LEN]) -> Result<Self, PanteonError> {
        Self::decode_slice(report)
    }
}

/// Decode a device read response buffer into a structured `DeviceGetReportResponse`.
pub fn decode_read_response(buf: &[u8]) -> Result<DeviceGetReportResponse, PanteonError> {
    DeviceGetReportResponse::decode_slice(buf)
}

/// Decode confirmed GET_REPORT response frame
pub fn decode_get_report_response(report: &[u8]) -> Result<DeviceGetReportResponse, PanteonError> {
    DeviceGetReportResponse::decode_slice(report)
}
