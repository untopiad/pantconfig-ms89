use crate::capture::extract::{extract_set_report_feature, Endian};
use crate::capture::assemble_snapshot;
use crate::model::StateSnapshot;
use crate::{PanteonError, FEATURE_REPORT_LEN};

pub(crate) fn parse_pcapng_internal(
    bytes: &[u8],
    pid: u16,
    label: &str,
) -> Result<StateSnapshot, PanteonError> {
    if bytes.len() < 12 {
        return Err(PanteonError::ProtocolDetailed {
            context: "parse_pcapng",
            detail: "pcapng data too short".to_string(),
        });
    }

    let mut endian = Endian::Little;
    let mut interfaces_ts_resol: Vec<f64> = Vec::new();
    let mut raw_frames: Vec<(f64, [u8; FEATURE_REPORT_LEN])> = Vec::new();

    let mut offset = 0;
    while offset + 8 <= bytes.len() {
        let block_type_raw = [
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ];

        // Section Header Block (SHB) magic 0x0A0D0D0A is symmetric under byte-order
        let is_shb = block_type_raw == [0x0a, 0x0d, 0x0d, 0x0a];
        let block_type = if is_shb {
            0x0a0d0d0a
        } else {
            endian.read_u32(&bytes[offset..offset + 4])
        };

        // Determine block length
        let block_len = if is_shb {
            // Tentatively read length using little-endian until BOM is checked
            u32::from_le_bytes([
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]) as usize
        } else {
            endian.read_u32(&bytes[offset + 4..offset + 8]) as usize
        };

        if block_len < 12 || offset + block_len > bytes.len() {
            return Err(PanteonError::ProtocolDetailed {
                context: "parse_pcapng",
                detail: format!("invalid or truncated block length {} at offset {}", block_len, offset),
            });
        }

        let body_len = block_len - 12;
        let body = &bytes[offset + 8..offset + 8 + body_len];
        let trailer_bytes = &bytes[offset + block_len - 4..offset + block_len];

        match block_type {
            0x0A0D0D0A => {
                // Section Header Block
                if body.len() < 4 {
                    return Err(PanteonError::ProtocolDetailed {
                        context: "parse_pcapng",
                        detail: "truncated Section Header Block".to_string(),
                    });
                }
                let bom = &body[0..4];
                if bom == [0x1a, 0x2b, 0x3c, 0x4d] {
                    endian = Endian::Big;
                } else if bom == [0x4d, 0x3c, 0x2b, 0x1a] {
                    endian = Endian::Little;
                } else {
                    return Err(PanteonError::ProtocolDetailed {
                        context: "parse_pcapng",
                        detail: "invalid byte order magic in Section Header Block".to_string(),
                    });
                }

                let trailer_len = endian.read_u32(trailer_bytes) as usize;
                if trailer_len != block_len {
                    return Err(PanteonError::ProtocolDetailed {
                        context: "parse_pcapng",
                        detail: "SHB trailer length mismatch".to_string(),
                    });
                }
                interfaces_ts_resol.clear();
            }
            0x00000001 => {
                // Interface Description Block (IDB)
                let trailer_len = endian.read_u32(trailer_bytes) as usize;
                if trailer_len != block_len {
                    return Err(PanteonError::ProtocolDetailed {
                        context: "parse_pcapng",
                        detail: "IDB trailer length mismatch".to_string(),
                    });
                }

                let mut resol = 1e-6; // Default resolution: 1 microsecond
                if body.len() >= 8 {
                    let mut opt_offset = 8; // skip LinkType (2), Reserved (2), SnapLen (4)
                    while opt_offset + 4 <= body.len() {
                        let opt_code = endian.read_u16(&body[opt_offset..opt_offset + 2]);
                        let opt_len = endian.read_u16(&body[opt_offset + 2..opt_offset + 4]) as usize;
                        let opt_val_offset = opt_offset + 4;
                        let opt_next = opt_val_offset + opt_len;
                        let pad = (4 - (opt_len % 4)) % 4;
                        opt_offset = opt_next + pad;

                        if opt_code == 0 {
                            break;
                        }
                        if opt_code == 9 && opt_len == 1 && opt_val_offset < body.len() {
                            let res_byte = body[opt_val_offset];
                            if (res_byte & 0x80) != 0 {
                                resol = 2.0_f64.powi(-((res_byte & 0x7f) as i32));
                            } else {
                                resol = 10.0_f64.powi(-(res_byte as i32));
                            }
                        }
                    }
                }
                interfaces_ts_resol.push(resol);
            }
            0x00000005 => {
                // Interface Statistics Block (ISB): standard metadata block, skip
                let trailer_len = endian.read_u32(trailer_bytes) as usize;
                if trailer_len != block_len {
                    return Err(PanteonError::ProtocolDetailed {
                        context: "parse_pcapng",
                        detail: "ISB trailer length mismatch".to_string(),
                    });
                }
            }
            0x00000006 => {
                // Enhanced Packet Block (EPB)
                let trailer_len = endian.read_u32(trailer_bytes) as usize;
                if trailer_len != block_len {
                    return Err(PanteonError::ProtocolDetailed {
                        context: "parse_pcapng",
                        detail: "EPB trailer length mismatch".to_string(),
                    });
                }

                if body.len() < 20 {
                    return Err(PanteonError::ProtocolDetailed {
                        context: "parse_pcapng",
                        detail: "truncated EPB header".to_string(),
                    });
                }

                let iface_id = endian.read_u32(&body[0..4]) as usize;
                let ts_h = endian.read_u32(&body[4..8]) as u64;
                let ts_l = endian.read_u32(&body[8..12]) as u64;
                let cap_len = endian.read_u32(&body[12..16]) as usize;

                if body.len() < 20 + cap_len {
                    return Err(PanteonError::ProtocolDetailed {
                        context: "parse_pcapng",
                        detail: "truncated EPB packet payload".to_string(),
                    });
                }

                let pkt = &body[20..20 + cap_len];
                let ts_raw = (ts_h << 32) | ts_l;
                let resol = interfaces_ts_resol.get(iface_id).copied().unwrap_or(1e-6);
                let ts_sec = (ts_raw as f64) * resol;

                if let Some(frame) = extract_set_report_feature(pkt, endian) {
                    raw_frames.push((ts_sec, frame));
                }
            }
            unsupported => {
                return Err(PanteonError::ProtocolDetailed {
                    context: "parse_pcapng",
                    detail: format!("unsupported pcapng block type: 0x{:08x}", unsupported),
                });
            }
        }

        offset += block_len;
    }

    if raw_frames.len() < 5 {
        return Err(PanteonError::ProtocolDetailed {
            context: "parse_pcapng",
            detail: format!(
                "capture contains too few SET_REPORT frames (found {}, expected at least 5)",
                raw_frames.len()
            ),
        });
    }

    assemble_snapshot(pid, label, &raw_frames)
}
