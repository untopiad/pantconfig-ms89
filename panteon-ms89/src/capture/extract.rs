use crate::{FEATURE_REPORT_LEN, REPORT_ID};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Endian {
    Little,
    Big,
}

impl Endian {
    pub(crate) fn read_u16(self, b: &[u8]) -> u16 {
        match self {
            Self::Little => u16::from_le_bytes([b[0], b[1]]),
            Self::Big => u16::from_be_bytes([b[0], b[1]]),
        }
    }

    pub(crate) fn read_u32(self, b: &[u8]) -> u32 {
        match self {
            Self::Little => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            Self::Big => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        }
    }
}

/// Extract 8-byte SET_REPORT feature frame from packet payload.
///
/// Recognizes Linux usbmon (64-byte / 48-byte header) and USBPcap headers,
/// rejecting any IN packets (such as mouse coordinate reports) and requiring
/// Class Host-to-Device request (0x21), bRequest SET_REPORT (0x09), and Report ID 0x07.
pub(crate) fn extract_set_report_feature(pkt: &[u8], endian: Endian) -> Option<[u8; FEATURE_REPORT_LEN]> {
    // 1. Linux usbmon (mmapped 64-byte header or regular 48-byte header)
    if pkt.len() >= 72 {
        let urb_type = pkt[8]; // 'S' = 0x53 (Submit)
        let ep = pkt[10];      // High bit indicates IN
        if urb_type == b'S' && (ep & 0x80 == 0) {
            let bm_req = pkt[40];
            let b_req = pkt[41];
            if bm_req == 0x21 && b_req == 0x09 {
                let payload = &pkt[64..];
                if payload.len() >= FEATURE_REPORT_LEN && payload[0] == REPORT_ID {
                    let mut frame = [0u8; FEATURE_REPORT_LEN];
                    frame.copy_from_slice(&payload[..FEATURE_REPORT_LEN]);
                    return Some(frame);
                }
            }
        }
    }

    // 2. Windows USBPcap header (header_len at offset 0)
    // Note: Host-to-device direction is established by bmRequestType == 0x21.
    // In newer USBPcap headers (len=28), offset 8 is part of the 64-bit IRP ID,
    // so checking pkt[8] & 0x80 falsely rejects valid packets.
    if pkt.len() >= 36 {
        let hdr_len = endian.read_u16(&pkt[0..2]) as usize;
        if (20..=pkt.len().saturating_sub(16)).contains(&hdr_len) {
            let bm_req = pkt[hdr_len];
            let b_req = pkt[hdr_len + 1];
            if bm_req == 0x21 && b_req == 0x09 {
                let payload = &pkt[hdr_len + 8..];
                if payload.len() >= FEATURE_REPORT_LEN && payload[0] == REPORT_ID {
                    let mut frame = [0u8; FEATURE_REPORT_LEN];
                    frame.copy_from_slice(&payload[..FEATURE_REPORT_LEN]);
                    return Some(frame);
                }
            }
        }
    }

    // 3. Raw USB setup frame fallback (8 bytes setup + 8 bytes data)
    if pkt.len() >= 16 && pkt[0] == 0x21 && pkt[1] == 0x09 && pkt[8] == REPORT_ID {
        let mut frame = [0u8; FEATURE_REPORT_LEN];
        frame.copy_from_slice(&pkt[8..16]);
        return Some(frame);
    }

    None
}
