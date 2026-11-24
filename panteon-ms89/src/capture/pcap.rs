use crate::capture::extract::{extract_set_report_feature, Endian};
use crate::capture::assemble_snapshot;
use crate::model::StateSnapshot;
use crate::{PanteonError, FEATURE_REPORT_LEN};

pub(crate) fn parse_pcap_classic_internal(
    bytes: &[u8],
    pid: u16,
    label: &str,
    endian: Endian,
    ts_resol: f64,
) -> Result<StateSnapshot, PanteonError> {
    if bytes.len() < 24 {
        return Err(PanteonError::ProtocolDetailed {
            context: "parse_pcap",
            detail: "classic pcap header truncated".to_string(),
        });
    }

    let mut offset = 24;
    let mut raw_frames: Vec<(f64, [u8; FEATURE_REPORT_LEN])> = Vec::new();

    while offset + 16 <= bytes.len() {
        let ts_sec = endian.read_u32(&bytes[offset..offset + 4]) as f64;
        let ts_frac = endian.read_u32(&bytes[offset + 4..offset + 8]) as f64;
        let incl_len = endian.read_u32(&bytes[offset + 8..offset + 12]) as usize;
        let _orig_len = endian.read_u32(&bytes[offset + 12..offset + 16]) as usize;
        offset += 16;

        if offset + incl_len > bytes.len() {
            return Err(PanteonError::ProtocolDetailed {
                context: "parse_pcap",
                detail: format!(
                    "truncated packet data (expected {} bytes at offset {})",
                    incl_len, offset
                ),
            });
        }

        let pkt = &bytes[offset..offset + incl_len];
        offset += incl_len;

        let ts = ts_sec + (ts_frac * ts_resol);
        if let Some(frame) = extract_set_report_feature(pkt, endian) {
            raw_frames.push((ts, frame));
        }
    }

    if raw_frames.is_empty() {
        return Err(PanteonError::ProtocolDetailed {
            context: "parse_pcap",
            detail: "capture contains no SET_REPORT frames".to_string(),
        });
    }

    assemble_snapshot(pid, label, &raw_frames)
}
