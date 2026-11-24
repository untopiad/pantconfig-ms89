pub mod extract;
pub mod pcap;
pub mod pcapng;

use crate::capture::extract::Endian;
use crate::capture::pcap::parse_pcap_classic_internal;
use crate::capture::pcapng::parse_pcapng_internal;
use crate::model::StateSnapshot;
use crate::{current_timestamp_secs, PanteonError, FEATURE_REPORT_LEN};

/// Parse PCAP or PCAPNG byte stream and extract PANTEON MS89 USB SET_REPORT feature reports.
///
/// Supports:
/// - Classic PCAP (magic `0xd4c3b2a1` / `0xa1b2c3d4` for microsecond resolution,
///   `0x4d3cb2a1` / `0xa1b23c4d` for nanosecond resolution).
/// - PCAPNG (Section Header Block magic `0x0a0d0d0a`).
pub fn parse_capture_feature_sets(
    bytes: &[u8],
    pid: u16,
    label: &str,
) -> Result<StateSnapshot, PanteonError> {
    if bytes.len() < 12 {
        return Err(PanteonError::ProtocolDetailed {
            context: "parse_capture",
            detail: "capture data too short".to_string(),
        });
    }

    let magic = &bytes[0..4];
    if magic == [0x0a, 0x0d, 0x0d, 0x0a] {
        parse_pcapng_internal(bytes, pid, label)
    } else if magic == [0xd4, 0xc3, 0xb2, 0xa1] {
        parse_pcap_classic_internal(bytes, pid, label, Endian::Little, 1e-6)
    } else if magic == [0xa1, 0xb2, 0xc3, 0xd4] {
        parse_pcap_classic_internal(bytes, pid, label, Endian::Big, 1e-6)
    } else if magic == [0x4d, 0x3c, 0xb2, 0xa1] {
        parse_pcap_classic_internal(bytes, pid, label, Endian::Little, 1e-9)
    } else if magic == [0xa1, 0xb2, 0x3c, 0x4d] {
        parse_pcap_classic_internal(bytes, pid, label, Endian::Big, 1e-9)
    } else {
        Err(PanteonError::ProtocolDetailed {
            context: "parse_capture",
            detail: "unsupported capture format".to_string(),
        })
    }
}

/// Backward compatibility alias for `parse_capture_feature_sets`.
pub fn parse_pcapng_feature_sets(
    bytes: &[u8],
    pid: u16,
    label: &str,
) -> Result<StateSnapshot, PanteonError> {
    parse_capture_feature_sets(bytes, pid, label)
}

pub(crate) fn assemble_snapshot(
    pid: u16,
    label: &str,
    raw_frames: &[(f64, [u8; FEATURE_REPORT_LEN])],
) -> Result<StateSnapshot, PanteonError> {
    let mut frames = Vec::with_capacity(raw_frames.len());
    let mut delays_ms = Vec::with_capacity(raw_frames.len());

    for (i, (ts_sec, frame)) in raw_frames.iter().enumerate() {
        frames.push(*frame);
        if i == 0 {
            delays_ms.push(0);
        } else {
            let prev_ts = raw_frames[i - 1].0;
            let delta = ((ts_sec - prev_ts) * 1000.0).max(0.0).round() as u64;
            delays_ms.push(delta.min(u16::MAX as u64) as u16);
        }
    }

    let captured_at = if !raw_frames.is_empty() && raw_frames[0].0 > 0.0 {
        (raw_frames[0].0 as u64).to_string()
    } else {
        current_timestamp_secs()
    };

    StateSnapshot::new(pid, captured_at, label, frames, delays_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn build_synthetic_pcapng(frames: &[[u8; 8]], ts_step_us: u64) -> Vec<u8> {
        let mut buf = Vec::new();

        // 1. SHB (Section Header Block)
        let mut shb_body = Vec::new();
        shb_body.extend_from_slice(&0x1a2b3c4du32.to_le_bytes()); // BOM (Little Endian)
        shb_body.extend_from_slice(&0x00010000u32.to_le_bytes()); // Major 1, Minor 0
        shb_body.extend_from_slice(&(-1i64).to_le_bytes()); // Section length unknown
        let shb_len = (12 + shb_body.len()) as u32;
        buf.extend_from_slice(&0x0a0d0d0au32.to_le_bytes());
        buf.extend_from_slice(&shb_len.to_le_bytes());
        buf.extend_from_slice(&shb_body);
        buf.extend_from_slice(&shb_len.to_le_bytes());

        // 2. IDB (Interface Description Block)
        let mut idb_body = Vec::new();
        idb_body.extend_from_slice(&220u16.to_le_bytes()); // LinkType (USB_LINUX_MMAPPED)
        idb_body.extend_from_slice(&0u16.to_le_bytes()); // Reserved
        idb_body.extend_from_slice(&65535u32.to_le_bytes()); // SnapLen
        // Option 9: tsresol = 6 (10^-6 s = 1 us)
        idb_body.extend_from_slice(&9u16.to_le_bytes()); // Option code 9
        idb_body.extend_from_slice(&1u16.to_le_bytes()); // Option len 1
        idb_body.push(6); // 10^-6 s
        idb_body.extend_from_slice(&[0, 0, 0]); // 3-byte padding
        // Option 0: End of options
        idb_body.extend_from_slice(&0u32.to_le_bytes());
        let idb_len = (12 + idb_body.len()) as u32;
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&idb_len.to_le_bytes());
        buf.extend_from_slice(&idb_body);
        buf.extend_from_slice(&idb_len.to_le_bytes());

        // 3. EPBs for each frame
        let mut current_ts = 1_000_000u64; // 1 second
        for frame in frames {
            let mut pkt = vec![0u8; 72];
            pkt[8] = b'S'; // URB Submit
            pkt[10] = 0x00; // Endpoint OUT
            pkt[40] = 0x21; // bmRequestType Class Host-to-Device
            pkt[41] = 0x09; // bRequest SET_REPORT
            pkt[42] = 0x07; // wValue Report ID 0x07
            pkt[43] = 0x03; // Feature
            pkt[64..72].copy_from_slice(frame);

            let ts_h = (current_ts >> 32) as u32;
            let ts_l = (current_ts & 0xffffffff) as u32;
            let cap_len = pkt.len() as u32;

            let mut epb_body = Vec::new();
            epb_body.extend_from_slice(&0u32.to_le_bytes()); // iface_id 0
            epb_body.extend_from_slice(&ts_h.to_le_bytes());
            epb_body.extend_from_slice(&ts_l.to_le_bytes());
            epb_body.extend_from_slice(&cap_len.to_le_bytes());
            epb_body.extend_from_slice(&cap_len.to_le_bytes());
            epb_body.extend_from_slice(&pkt);

            let epb_len = (12 + epb_body.len()) as u32;
            buf.extend_from_slice(&6u32.to_le_bytes());
            buf.extend_from_slice(&epb_len.to_le_bytes());
            buf.extend_from_slice(&epb_body);
            buf.extend_from_slice(&epb_len.to_le_bytes());

            current_ts += ts_step_us;
        }

        buf
    }

    fn build_synthetic_pcap_classic(frames: &[[u8; 8]], ts_step_us: u32) -> Vec<u8> {
        let mut buf = Vec::new();
        // 24 bytes global header
        buf.extend_from_slice(&[0xd4, 0xc3, 0xb2, 0xa1]); // Magic (LE, us)
        buf.extend_from_slice(&2u16.to_le_bytes()); // Major 2
        buf.extend_from_slice(&4u16.to_le_bytes()); // Minor 4
        buf.extend_from_slice(&0i32.to_le_bytes()); // thiszone
        buf.extend_from_slice(&0u32.to_le_bytes()); // sigfigs
        buf.extend_from_slice(&65535u32.to_le_bytes()); // snaplen
        buf.extend_from_slice(&249u32.to_le_bytes()); // LinkType USBPcap

        let mut current_ts_sec = 1_700_000_000u32;
        let mut current_ts_usec = 100_000u32;

        for frame in frames {
            // USBPcap packet: 28 bytes header + 8 bytes setup + 8 bytes data = 44 bytes
            let mut pkt = vec![0u8; 44];
            pkt[0..2].copy_from_slice(&28u16.to_le_bytes()); // hdr_len = 28
            // SETUP at offset 28
            pkt[28] = 0x21; // bmRequestType
            pkt[29] = 0x09; // bRequest SET_REPORT
            pkt[30] = 0x07; // wValue Report ID
            pkt[31] = 0x03; // Feature
            // Data at offset 36
            pkt[36..44].copy_from_slice(frame);

            let cap_len = pkt.len() as u32;
            buf.extend_from_slice(&current_ts_sec.to_le_bytes());
            buf.extend_from_slice(&current_ts_usec.to_le_bytes());
            buf.extend_from_slice(&cap_len.to_le_bytes());
            buf.extend_from_slice(&cap_len.to_le_bytes());
            buf.extend_from_slice(&pkt);

            current_ts_usec += ts_step_us;
            if current_ts_usec >= 1_000_000 {
                current_ts_sec += current_ts_usec / 1_000_000;
                current_ts_usec %= 1_000_000;
            }
        }
        buf
    }

    #[test]
    fn test_parse_pcapng_golden_synthetic() {
        let test_frames = vec![
            [0x07, 0x18, 0x03, 0x00, 0x01, 0x00, 0x03, 0x00],
            [0x07, 0x18, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00],
            [0x07, 0x18, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00],
            [0x07, 0x18, 0x03, 0x06, 0x18, 0x00, 0x00, 0x07],
            [0x07, 0x18, 0x09, 0x00, 0x18, 0x00, 0x00, 0x07],
        ];

        let pcapng_bytes = build_synthetic_pcapng(&test_frames, 20_000); // 20ms step
        let snapshot = parse_pcapng_feature_sets(&pcapng_bytes, 0x1440, "golden_test").unwrap();

        assert_eq!(snapshot.source_pid, 0x1440);
        assert_eq!(snapshot.label, "golden_test");
        assert_eq!(snapshot.frames.len(), 5);
        assert_eq!(snapshot.delays_ms.len(), 5);
        assert_eq!(snapshot.delays_ms[0], 0);
        assert_eq!(snapshot.delays_ms[1], 20);
        assert_eq!(snapshot.delays_ms[2], 20);
        assert_eq!(snapshot.delays_ms[3], 20);
        assert_eq!(snapshot.delays_ms[4], 20);
        assert_eq!(snapshot.frames, test_frames);
    }

    #[test]
    fn test_parse_classic_pcap_synthetic_golden() {
        let test_frames = vec![
            [0x07, 0x18, 0x03, 0x00, 0x01, 0x00, 0x03, 0x00],
            [0x07, 0x18, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00],
        ];

        let pcap_bytes = build_synthetic_pcap_classic(&test_frames, 25_000); // 25ms step
        let snapshot = parse_capture_feature_sets(&pcap_bytes, 0x1440, "golden_classic").unwrap();

        assert_eq!(snapshot.source_pid, 0x1440);
        assert_eq!(snapshot.label, "golden_classic");
        assert_eq!(snapshot.frames.len(), 2);
        assert_eq!(snapshot.delays_ms.len(), 2);
        assert_eq!(snapshot.delays_ms[0], 0);
        assert_eq!(snapshot.delays_ms[1], 25);
        assert_eq!(snapshot.frames, test_frames);
    }

    #[test]
    fn test_parse_capture_negative_unsupported_magic() {
        let corrupt = vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x00, 0x00, 0x00, 0x00];
        let res = parse_capture_feature_sets(&corrupt, 0x1440, "bad_magic");
        assert!(res.is_err());
        match res.unwrap_err() {
            PanteonError::ProtocolDetailed { context, detail } => {
                assert_eq!(context, "parse_capture");
                assert!(detail.contains("unsupported capture format"));
            }
            err => panic!("Unexpected error: {:?}", err),
        }
    }

    #[test]
    fn test_parse_pcapng_too_few_frames_fails() {
        let test_frames = vec![
            [0x07, 0x18, 0x03, 0x00, 0x01, 0x00, 0x03, 0x00],
            [0x07, 0x18, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00],
        ];

        let pcapng_bytes = build_synthetic_pcapng(&test_frames, 10_000);
        let res = parse_pcapng_feature_sets(&pcapng_bytes, 0x1440, "few_frames");
        assert!(res.is_err());
        match res.unwrap_err() {
            PanteonError::ProtocolDetailed { context, detail } => {
                assert_eq!(context, "parse_pcapng");
                assert!(detail.contains("capture contains too few SET_REPORT frames"));
            }
            err => panic!("Unexpected error: {:?}", err),
        }
    }

    #[test]
    fn test_parse_pcapng_invalid_magic_fails() {
        let corrupt = vec![0x00, 0x01, 0x02, 0x03, 0x0c, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00];
        let res = parse_pcapng_feature_sets(&corrupt, 0x1440, "corrupt");
        assert!(res.is_err());
    }

    #[test]
    fn test_parse_pcapng_unsupported_block_type_fails() {
        let mut buf = build_synthetic_pcapng(&[
            [0x07, 0x18, 0x03, 0x00, 0x01, 0x00, 0x03, 0x00],
            [0x07, 0x18, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00],
            [0x07, 0x18, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00],
            [0x07, 0x18, 0x03, 0x06, 0x18, 0x00, 0x00, 0x07],
            [0x07, 0x18, 0x09, 0x00, 0x18, 0x00, 0x00, 0x07],
        ], 10_000);

        // Append unsupported block type 0x0000000a (Decryption Secrets Block)
        let block_len = 16u32;
        buf.extend_from_slice(&0x0000000au32.to_le_bytes());
        buf.extend_from_slice(&block_len.to_le_bytes());
        buf.extend_from_slice(&[0u8; 4]);
        buf.extend_from_slice(&block_len.to_le_bytes());

        let res = parse_pcapng_feature_sets(&buf, 0x1440, "unsupported");
        assert!(res.is_err());
        match res.unwrap_err() {
            PanteonError::ProtocolDetailed { context, detail } => {
                assert_eq!(context, "parse_pcapng");
                assert!(detail.contains("unsupported pcapng block type: 0x0000000a"));
            }
            err => panic!("Unexpected error: {:?}", err),
        }
    }

    #[test]
    fn test_regression_fixtures_breath_and_reset() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let pcap_path = manifest_dir.join("tests/fixtures/breath_effect.pcap");
        let pcapng_path = manifest_dir.join("tests/fixtures/breath_effect.pcapng");
        let reset_path = manifest_dir.join("tests/fixtures/reset_sample.pcapng");

        let pcap_bytes = std::fs::read(&pcap_path).expect("read breath_effect.pcap");
        let snap_pcap = parse_capture_feature_sets(&pcap_bytes, 0x1440, "breath_pcap").expect("parse breath pcap");
        assert!(snap_pcap.frames.len() > 100, "expected >100 frames, found {}", snap_pcap.frames.len());

        let pcapng_bytes = std::fs::read(&pcapng_path).expect("read breath_effect.pcapng");
        let snap_pcapng = parse_capture_feature_sets(&pcapng_bytes, 0x1440, "breath_pcapng").expect("parse breath pcapng");
        assert_eq!(snap_pcap.frames, snap_pcapng.frames, "pcap and pcapng frames must match byte-for-byte");

        let reset_bytes = std::fs::read(&reset_path).expect("read reset_sample.pcapng");
        let snap_reset = parse_capture_feature_sets(&reset_bytes, 0x1440, "reset_pcapng").expect("parse reset pcapng");
        assert!(snap_reset.frames.len() > 80, "expected >80 frames, found {}", snap_reset.frames.len());
    }
}
