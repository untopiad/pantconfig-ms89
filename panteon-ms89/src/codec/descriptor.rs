use crate::PanteonError;

/// Parse a raw HID Report Descriptor and find the declared data size (in bytes) of a given feature report ID.
pub fn parse_hid_feature_report_size(desc: &[u8], target_report_id: u8) -> Option<usize> {
    let mut i = 0;
    let mut curr_report_id = None;
    let mut curr_size: usize = 0;
    let mut curr_count: usize = 0;
    let mut total_bits: usize = 0;

    while i < desc.len() {
        let b = desc[i];
        if b == 0xFE {
            // Long item
            if i + 1 >= desc.len() {
                break;
            }
            let data_len = desc[i + 1] as usize;
            i += 3 + data_len;
            continue;
        }

        let size_code = (b & 0x03) as usize;
        let data_len = if size_code == 3 { 4 } else { size_code };
        let b_type = (b >> 2) & 0x03;
        let b_tag = (b >> 4) & 0x0F;

        let mut data: u32 = 0;
        if i + 1 + data_len <= desc.len() {
            for (shift, d_idx) in (i + 1..i + 1 + data_len).enumerate() {
                data |= (desc[d_idx] as u32) << (shift * 8);
            }
        }

        if b_type == 1 {
            // Global item
            if b_tag == 7 {
                curr_size = data as usize;
            } else if b_tag == 8 {
                curr_report_id = Some(data as u8);
            } else if b_tag == 9 {
                curr_count = data as usize;
            }
        } else if b_type == 0 {
            // Main item
            if b_tag == 11 {
                // Feature item
                if curr_report_id == Some(target_report_id) {
                    total_bits += curr_size * curr_count;
                }
            }
        }

        i += 1 + data_len;
    }

    if total_bits > 0 {
        Some(total_bits.div_ceil(8))
    } else {
        None
    }
}

/// Represents decoded 32-byte descriptor page (bank 0, address 0x0000..=0x001f).
///
/// # Field Confidence & Stability (M3.2 Finding)
/// Live hardware experiments show that the descriptor page is **session-dynamic**:
/// its contents are 100% deterministic and reproducible within an active session,
/// but vary across USB power-cycles / replugs on the same physical device and PID.
///
/// Only two fields have confirmed structural semantics across sessions:
/// - Offset 0: operation echo / ACK marker (0x5a probe echo, 0x03 transaction ACK echo) [CONFIRMED]
/// - Offset 6: data_lo echo of last written transaction (dynamic write echo) [CONFIRMED]
///
/// All other field interpretations (sync bytes, unlock_state, PID, vid_lo, bank flags,
/// and calibration bytes) are per-session observations and must be treated as low-confidence:
/// - Offset 0: sync byte / operation echo (0x5a init, 0x03 ACK echo post-write) [CONFIRMED]
/// - Offset 1: unlock state candidate [per-session]
/// - Offset 2: sync byte candidate (e.g. 0xa5) [per-session]
/// - Offset 3..5: PID candidate (little-endian u16) [per-session]
/// - Offset 5: static candidate (VID_LO candidate) [per-session]
/// - Offset 6: dynamic write echo byte (echo of data_lo from last write) [CONFIRMED]
/// - Offset 7: flags / status candidate [per-session]
/// - Offset 8..16: 8 bank identifiers / flags [per-session]
/// - Offset 16..32: 16 calibration / payload bytes [per-session]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DescriptorPage {
    pub sync: (u8, u8),
    pub unlock_state: u8,
    pub pid: u16,
    pub vid_lo: u8,       // byte 5: статический байт (кандидат VID_LO = 0xfa)
    pub write_echo: u8,   // byte 6: динамический, эхо data_lo последней write-транзакции
    pub flags: u8,
    pub bank_flags: [u8; 8],
    pub calibration: [u8; 16],
    pub raw: [u8; 32],
}

impl DescriptorPage {
    /// Format descriptor page as hex table and field interpretation
    pub fn format_table(&self) -> String {
        let mut out = String::new();
        out.push_str("Descriptor Page (32 bytes):\n");
        out.push_str(&format!(
            "  0000: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}  {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}\n",
            self.raw[0], self.raw[1], self.raw[2], self.raw[3], self.raw[4], self.raw[5], self.raw[6], self.raw[7],
            self.raw[8], self.raw[9], self.raw[10], self.raw[11], self.raw[12], self.raw[13], self.raw[14], self.raw[15],
        ));
        out.push_str(&format!(
            "  0010: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}  {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}\n\n",
            self.raw[16], self.raw[17], self.raw[18], self.raw[19], self.raw[20], self.raw[21], self.raw[22], self.raw[23],
            self.raw[24], self.raw[25], self.raw[26], self.raw[27], self.raw[28], self.raw[29], self.raw[30], self.raw[31],
        ));
        out.push_str("Interpretation:\n");
        out.push_str(&format!("  Sync bytes:      0x{:02x}, 0x{:02x}\n", self.sync.0, self.sync.1));
        out.push_str(&format!("  Unlock state:    0x{:02x}\n", self.unlock_state));
        out.push_str(&format!("  PID:             0x{:04x}\n", self.pid));
        out.push_str(&format!(
            "  VID_LO / Echo:   0x{:02x} / 0x{:02x} (vid_lo=0x{:02x} static, write_echo=0x{:02x} last write data_lo)\n",
            self.vid_lo, self.write_echo, self.vid_lo, self.write_echo
        ));
        out.push_str(&format!("  Flags (byte 7):  0x{:02x}\n", self.flags));
        out.push_str(&format!(
            "  Bank flags (8):  [{}]\n",
            self.bank_flags.iter().map(|b| format!("0x{:02x}", b)).collect::<Vec<_>>().join(", ")
        ));
        for (i, b) in self.bank_flags.iter().enumerate() {
            out.push_str(&format!("    Bank {}: 0x{:02x}\n", i, b));
        }
        out.push_str(&format!(
            "  Calibration:     {}\n",
            self.calibration.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
        ));
        out
    }

    /// Serialize descriptor page to typed JSON string
    pub fn to_json_string(&self) -> String {
        let bank_strs: Vec<String> = self.bank_flags.iter().map(|b| format!("\"0x{:02x}\"", b)).collect();
        let cal_strs: Vec<String> = self.calibration.iter().map(|b| format!("\"0x{:02x}\"", b)).collect();
        let raw_strs: Vec<String> = self.raw.iter().map(|b| format!("\"0x{:02x}\"", b)).collect();
        format!(
            "{{\n  \"sync\": [\"0x{:02x}\", \"0x{:02x}\"],\n  \"unlock_state\": \"0x{:02x}\",\n  \"pid\": \"0x{:04x}\",\n  \"vid_lo\": \"0x{:02x}\",\n  \"write_echo\": \"0x{:02x}\",\n  \"flags\": \"0x{:02x}\",\n  \"bank_flags\": [{}],\n  \"calibration\": [{}],\n  \"raw_bytes\": [{}]\n}}\n",
            self.sync.0, self.sync.1,
            self.unlock_state,
            self.pid,
            self.vid_lo,
            self.write_echo,
            self.flags,
            bank_strs.join(", "),
            cal_strs.join(", "),
            raw_strs.join(", ")
        )
    }
}

/// Known live descriptor page capture from sweep_base0.json (ground truth golden vector).
pub const LIVE_DESCRIPTOR_PAGE_BYTES: [u8; 32] = [
    0x5a, 0x03, 0xa5, 0x40, 0x14, 0xfa, 0x30, 0x10,
    0x21, 0x22, 0x23, 0x24, 0x25, 0x2c, 0x29, 0x2b,
    0xff, 0xa1, 0x34, 0xcb, 0xef, 0x1f, 0xf2, 0xfb,
    0xfc, 0xc7, 0xe7, 0xbd, 0x77, 0xae, 0x7f, 0xff,
];

/// Known post-write descriptor page capture after setting 1000 Hz polling rate (M2.1/M2.2 experiment 5).
/// Demonstrates byte 0 = 0x03 (ACK echo) and byte 6 = 0x00 (echo of data_lo 0x00).
pub const POST_WRITE_DESCRIPTOR_PAGE_BYTES: [u8; 32] = [
    0x03, 0x03, 0xa5, 0x40, 0x14, 0xfa, 0x00, 0x10,
    0x21, 0x22, 0x23, 0x24, 0x25, 0x2c, 0x29, 0x2b,
    0xff, 0xa1, 0x34, 0xcb, 0xef, 0x1f, 0xf2, 0xfb,
    0xfc, 0xc7, 0xe7, 0xbd, 0x77, 0xae, 0x7f, 0xff,
];

/// Decode 32-byte descriptor page buffer into typed DescriptorPage structure.
pub fn decode_descriptor_page(bytes: &[u8]) -> Result<DescriptorPage, PanteonError> {
    if bytes.len() != 32 {
        return Err(PanteonError::Protocol("descriptor page length must be exactly 32 bytes"));
    }
    let mut raw = [0u8; 32];
    raw.copy_from_slice(bytes);

    let sync = (bytes[0], bytes[2]);
    let unlock_state = bytes[1];
    let pid = u16::from_le_bytes([bytes[3], bytes[4]]);
    let vid_lo = bytes[5];
    let write_echo = bytes[6];
    let flags = bytes[7];

    let mut bank_flags = [0u8; 8];
    bank_flags.copy_from_slice(&bytes[8..16]);

    let mut calibration = [0u8; 16];
    calibration.copy_from_slice(&bytes[16..32]);

    Ok(DescriptorPage {
        sync,
        unlock_state,
        pid,
        vid_lo,
        write_echo,
        flags,
        bank_flags,
        calibration,
        raw,
    })
}
