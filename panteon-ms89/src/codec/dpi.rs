//! DPI slot encoder, decoder, and color PWM model (M3.5 / M4.0).

use crate::device::WriteTransactionPlan;
use crate::PanteonError;

/// 23 discrete calibrated DPI steps supported by PANTEON MS89 hardware.
pub const DPI_TABLE: [u32; 23] = [
    200, 400, 600, 800, 1000, 1200, 1400, 1600, 1800, 2000,
    2400, 3200, 4000, 4800, 5600, 6400, 7200, 8000, 8800, 9600,
    10400, 11200, 12800,
];

/// Factory default DPI values for the 6 slots.
pub const DEFAULT_DPI_SLOTS: [u32; 6] = [200, 400, 800, 1600, 3200, 6400];

/// Factory default RGB colors for the 6 slots:
/// Slot 1: Yellow (255, 255, 0)
/// Slot 2: Blue   (0, 0, 255)
/// Slot 3: Magenta(255, 0, 255)
/// Slot 4: Green  (0, 255, 0)
/// Slot 5: Red    (255, 0, 0)
/// Slot 6: Cyan   (0, 255, 255)
pub const DEFAULT_SLOT_COLORS: [(u8, u8, u8); 6] = [
    (255, 255, 0),
    (0, 0, 255),
    (255, 0, 255),
    (0, 255, 0),
    (255, 0, 0),
    (0, 255, 255),
];

/// Map a DPI value to its 5-bit hardware register code.
pub fn dpi_to_code(dpi: u32) -> Result<u8, PanteonError> {
    match dpi {
        200 => Ok(0x01),
        400 => Ok(0x02),
        600 => Ok(0x03),
        800 => Ok(0x04),
        1000 => Ok(0x05),
        1200 => Ok(0x06),
        1400 => Ok(0x07),
        1600 => Ok(0x08),
        1800 => Ok(0x09),
        2000 => Ok(0x0a),
        2400 => Ok(0x0c),
        3200 => Ok(0x0d),
        4000 => Ok(0x0e),
        4800 => Ok(0x0f),
        5600 => Ok(0x10),
        6400 => Ok(0x11),
        7200 => Ok(0x12),
        8000 => Ok(0x13),
        8800 => Ok(0x14),
        9600 => Ok(0x15),
        10400 => Ok(0x16),
        11200 => Ok(0x17),
        12800 => Ok(0x18),
        _ => Err(PanteonError::ProtocolDetailed {
            context: "dpi_to_code",
            detail: format!(
                "Invalid DPI: {}. Allowed discrete steps: 200..2000 (step 200), 2400..7200 (step 800), 8000..11200 (step 800), 12800",
                dpi
            ),
        }),
    }
}

/// Map a 5-bit hardware register code back to DPI value.
pub fn code_to_dpi(code: u8) -> Result<u32, PanteonError> {
    match code {
        0x01 => Ok(200),
        0x02 => Ok(400),
        0x03 => Ok(600),
        0x04 => Ok(800),
        0x05 => Ok(1000),
        0x06 => Ok(1200),
        0x07 => Ok(1400),
        0x08 => Ok(1600),
        0x09 => Ok(1800),
        0x0a => Ok(2000),
        0x0c => Ok(2400),
        0x0d => Ok(3200),
        0x0e => Ok(4000),
        0x0f => Ok(4800),
        0x10 => Ok(5600),
        0x11 => Ok(6400),
        0x12 => Ok(7200),
        0x13 => Ok(8000),
        0x14 => Ok(8800),
        0x15 => Ok(9600),
        0x16 => Ok(10400),
        0x17 => Ok(11200),
        0x18 => Ok(12800),
        _ => Err(PanteonError::ProtocolDetailed {
            context: "code_to_dpi",
            detail: format!("Unknown DPI register code: 0x{:02x}", code),
        }),
    }
}

/// Encode 6 DPI values into 8 register write plans (Banks 0..7 @ Addr 0x0010).
pub fn encode_dpi_slots(slots: &[u32; 6]) -> Result<[WriteTransactionPlan; 8], PanteonError> {
    let mut codes = [0u8; 6];
    for (i, &dpi) in slots.iter().enumerate() {
        codes[i] = dpi_to_code(dpi)?;
    }

    // Bank 3: Bit 4 mask (high DPI mode for slots >= 5600 DPI)
    let mut b3 = 0u8;
    for (i, &c) in codes.iter().enumerate() {
        b3 |= ((c >> 4) & 1) << i;
    }

    // Bank 5: lower nibbles of Slot 1 and Slot 2
    let b5 = ((codes[0] & 0x0f) << 4) | (codes[1] & 0x0f);

    // Bank 6: lower nibbles of Slot 3 and Slot 4
    let b6 = ((codes[2] & 0x0f) << 4) | (codes[3] & 0x0f);

    // Bank 7: lower nibbles of Slot 5 and Slot 6
    let b7 = ((codes[4] & 0x0f) << 4) | (codes[5] & 0x0f);

    Ok([
        WriteTransactionPlan::new(0, 0x0010, 0x0700),
        WriteTransactionPlan::new(1, 0x0010, 0x0700),
        WriteTransactionPlan::new(2, 0x0010, 0x0700),
        WriteTransactionPlan::new(3, 0x0010, 0x0700 | (b3 as u16)),
        WriteTransactionPlan::new(4, 0x0010, 0x0708),
        WriteTransactionPlan::new(5, 0x0010, 0x0700 | (b5 as u16)),
        WriteTransactionPlan::new(6, 0x0010, 0x0700 | (b6 as u16)),
        WriteTransactionPlan::new(7, 0x0010, 0x0700 | (b7 as u16)),
    ])
}

/// Decode raw register byte values (Bank 3, 5, 6, 7 @ 0x0010) into 6 DPI values.
pub fn decode_dpi_slots(b3: u8, b5: u8, b6: u8, b7: u8) -> Result<[u32; 6], PanteonError> {
    let codes = [
        ((b3 & 0x01) << 4) | (b5 >> 4),
        (((b3 >> 1) & 0x01) << 4) | (b5 & 0x0f),
        (((b3 >> 2) & 0x01) << 4) | (b6 >> 4),
        (((b3 >> 3) & 0x01) << 4) | (b6 & 0x0f),
        (((b3 >> 4) & 0x01) << 4) | (b7 >> 4),
        (((b3 >> 5) & 0x01) << 4) | (b7 & 0x0f),
    ];

    let mut slots = [0u32; 6];
    for (i, &code) in codes.iter().enumerate() {
        slots[i] = code_to_dpi(code)?;
    }
    Ok(slots)
}

/// Inverted active-low 4-bit PWM conversion for an 8-bit color channel.
/// `(255 - channel) >> 4`.
#[inline]
pub fn rgb_to_pwm(channel: u8) -> u8 {
    (255 - channel) >> 4
}

/// 4-bit PWM nibble to 8-bit color channel reconstruction.
#[inline]
pub fn pwm_to_rgb(pwm: u8) -> u8 {
    (15 - (pwm & 0x0f)) * 17
}

/// Factory default secondary RGB colors for the 6 slots (corresponding to OEM high-nibble state):
/// Slot 1: (255, 238, 0) -> PWM (0, 1, 15) -> hi (0x0, 0x1, 0xf)
/// Slot 2: (0, 119, 255) -> PWM (15, 8, 0)  -> hi (0xf, 0x8, 0x0)
/// Slot 3: (255, 0, 119) -> PWM (0, 15, 8)  -> hi (0x0, 0xf, 0x8)
/// Slot 4: (0, 255, 68)  -> PWM (15, 0, 11) -> hi (0xf, 0x0, 0xb)
/// Slot 5: (255, 0, 0)   -> PWM (0, 15, 15) -> hi (0x0, 0xf, 0xf)
/// Slot 6: (0, 255, 255) -> PWM (15, 0, 0)  -> hi (0xf, 0x0, 0x0)
pub const DEFAULT_SECOND_COLORS: [(u8, u8, u8); 6] = [
    (255, 238, 0),
    (0, 119, 255),
    (255, 0, 119),
    (0, 255, 68),
    (255, 0, 0),
    (0, 255, 255),
];

/// Former frozen high nibble constants, superseded by two-state nibble model (ADR-003).
/// Retained for reference and backward compatibility.
#[deprecated(
    since = "0.2.0",
    note = "High nibbles are dynamic PWM from second_colors state, see ADR-003"
)]
pub const COLOR_CHANNEL_HIGH_NIBBLES: [u8; 18] = [
    0x0, 0x1, 0xf, 0xf, 0x8, 0x0, 0x0, 0xf, // Addr 0x0020, Banks 0..7
    0x8, 0xf, 0x0, 0xb, 0x0, 0xf, 0xf, 0xf, // Addr 0x0028, Banks 0..7
    0x0, 0x0,                               // Addr 0x0030, Banks 0..1
];

pub use crate::model::registers::LED_BRIGHTNESS;

/// Convert brightness level (0..=8) to Bank 7 @ 0x0038 register data value.
/// Active-low encoding: 0x0700 | (((8 - level) as u16) << 4).
pub fn brightness_to_bank7(level: u8) -> Result<u16, PanteonError> {
    if level > 8 {
        return Err(PanteonError::ProtocolDetailed {
            context: "brightness_to_bank7",
            detail: format!("LED brightness level must be 0..=8, got {}", level),
        });
    }
    Ok(0x0700 | (((8 - level) as u16) << 4))
}

/// Extract brightness level (0..=8) from Bank 7 @ 0x0038 register data value.
pub fn bank7_to_brightness(val: u16) -> Result<u8, PanteonError> {
    let lo = (val & 0xff) as u8;
    let nibble = (lo >> 4) & 0x0f;
    if nibble > 8 {
        return Err(PanteonError::ProtocolDetailed {
            context: "bank7_to_brightness",
            detail: format!("Invalid Bank 7 nibble 0x{:x} for brightness", nibble),
        });
    }
    Ok(8 - nibble)
}

/// Normalizes a wire brightness level to its canonical editor value.
///
/// Нижний ниббл `8` (wire-уровень `0`, слово `0x0780`) лежит вне OEM-домена
/// `0..=7` и на железе светит как 100% (живое наблюдение `USER_LIVE_R20`).
/// Нормализуем его в уровень `8` — единственный канонический
/// 100%; остальные уровни возвращаются как есть.
///
/// Это функция семантики редактора: замороженный wire-контракт
/// [`brightness_to_bank7`]/[`bank7_to_brightness`] она не меняет.
pub fn normalize_brightness_level(level: u8) -> u8 {
    if level == 0 {
        8
    } else {
        level
    }
}

/// Semantic brightness percentage for a wire level `0..=8` (total function).
///
/// - `0` → `100.0` — ниббл `8` вне OEM-домена, светит как 100% (`USER_LIVE_R20`);
/// - `1` → `0.0` — полное отключение подсветки (`USER_LIVE_R20`);
/// - `L >= 2` → `L · 12.5` (25% … 100%).
pub fn brightness_percent(level: u8) -> f64 {
    match level {
        0 => 100.0,
        1 => 0.0,
        _ => level as f64 * 12.5,
    }
}

/// Encode block 0x0038 register write plans (8 banks).
/// Confirmed in OEM traffic: bank 2 = 0x0a, bank 6 = 0x8f, bank 7 = global brightness (level 0..=8).
pub fn encode_color_block_38(brightness: u8) -> Result<[WriteTransactionPlan; 8], PanteonError> {
    let b7 = brightness_to_bank7(brightness)?;
    encode_color_block_38_raw(b7)
}

/// Encode block 0x0038 register write plans with raw bank 7 value.
pub fn encode_color_block_38_raw(bank7_val: u16) -> Result<[WriteTransactionPlan; 8], PanteonError> {
    let bytes_38: [u16; 8] = [0x0700, 0x0700, 0x070a, 0x0700, 0x0700, 0x0700, 0x078f, bank7_val];
    Ok([
        WriteTransactionPlan::new(0, 0x0038, bytes_38[0]),
        WriteTransactionPlan::new(1, 0x0038, bytes_38[1]),
        WriteTransactionPlan::new(2, 0x0038, bytes_38[2]),
        WriteTransactionPlan::new(3, 0x0038, bytes_38[3]),
        WriteTransactionPlan::new(4, 0x0038, bytes_38[4]),
        WriteTransactionPlan::new(5, 0x0038, bytes_38[5]),
        WriteTransactionPlan::new(6, 0x0038, bytes_38[6]),
        WriteTransactionPlan::new(7, 0x0038, bytes_38[7]),
    ])
}

/// Encode 6 slot RGB colors into 32 register write plans across addresses 0x0020, 0x0028, 0x0030, and 0x0038.
///
/// Both main and secondary color channels are quantized via `DpiSlot::to_pwm_bytes()`.
/// Each byte is composed as: `(pwm(second[c]) << 4) | (pwm(main[c]) & 0x0f)`.
/// Addr 0x0030 writes all 8 banks (banks 2..7 containing wire values 0xff, 0x7f, 0x00, 0x00, 0x00, 0x00).
/// Addr 0x0038 writes all 8 banks via `encode_color_block_38`.
pub fn encode_slot_colors(
    colors: &[(u8, u8, u8); 6],
    second_colors: &[(u8, u8, u8); 6],
    brightness: u8,
) -> Result<Vec<WriteTransactionPlan>, PanteonError> {
    let slots: [crate::model::DpiSlot; 6] = std::array::from_fn(|i| crate::model::DpiSlot {
        level_index: (i + 1) as u8,
        dpi: 0,
        enabled: true,
        main: [colors[i].0, colors[i].1, colors[i].2],
        second: [second_colors[i].0, second_colors[i].1, second_colors[i].2],
    });
    encode_slots_colors(&slots, brightness)
}

/// Encode 6 DPI slots directly using `DpiSlot::to_pwm_bytes()` into 32 register write plans.
pub fn encode_slots_colors(
    slots: &[crate::model::DpiSlot; 6],
    brightness: u8,
) -> Result<Vec<WriteTransactionPlan>, PanteonError> {
    let mut channels = [0u8; 18];
    for (slot, s) in slots.iter().enumerate() {
        let pwm = s.to_pwm_bytes();
        channels[slot * 3] = pwm[0];
        channels[slot * 3 + 1] = pwm[1];
        channels[slot * 3 + 2] = pwm[2];
    }

    let mut plans = Vec::with_capacity(32);

    // Addr 0x0020, Banks 0..7 (channels 0..8)
    for bank in 0..8u8 {
        plans.push(WriteTransactionPlan::new(
            bank,
            0x0020,
            0x0700 | (channels[bank as usize] as u16),
        ));
    }

    // Addr 0x0028, Banks 0..7 (channels 8..16)
    for bank in 0..8u8 {
        plans.push(WriteTransactionPlan::new(
            bank,
            0x0028,
            0x0700 | (channels[8 + bank as usize] as u16),
        ));
    }

    // Addr 0x0030, Banks 0..7:
    // Banks 0..1: channels 16..17 (Slot 5 G & B)
    // Banks 2..7: compose-bytes from OEM trace: 0xff, 0x7f, 0x00, 0x00, 0x00, 0x00
    let b0_30 = channels[16];
    let b1_30 = channels[17];
    let bytes_30: [u8; 8] = [b0_30, b1_30, 0xff, 0x7f, 0x00, 0x00, 0x00, 0x00];
    for bank in 0..8u8 {
        plans.push(WriteTransactionPlan::new(
            bank,
            0x0030,
            0x0700 | (bytes_30[bank as usize] as u16),
        ));
    }

    // Addr 0x0038, Banks 0..7
    plans.extend(encode_color_block_38(brightness)?);

    Ok(plans)
}

/// Encode full color configuration session (32 write plans across 0x0020, 0x0028, 0x0030, 0x0038).
#[inline]
pub fn encode_color_session(
    main: &[(u8, u8, u8); 6],
    second: &[(u8, u8, u8); 6],
    brightness: u8,
) -> Result<Vec<WriteTransactionPlan>, PanteonError> {
    encode_slot_colors(main, second, brightness)
}

/// Decode 18 channel register values from Addr 0x0020 (8 bytes), Addr 0x0028 (8 bytes), and Addr 0x0030 (2 bytes).
pub fn decode_slot_colors(
    bytes_0x20: &[u8; 8],
    bytes_0x28: &[u8; 8],
    bytes_0x30: &[u8; 2],
) -> [(u8, u8, u8); 6] {
    let mut ch = [0u8; 18];
    ch[..8].copy_from_slice(bytes_0x20);
    ch[8..16].copy_from_slice(bytes_0x28);
    ch[16..18].copy_from_slice(bytes_0x30);

    let mut colors = [(0u8, 0u8, 0u8); 6];
    for i in 0..6 {
        let r = pwm_to_rgb(ch[i * 3] & 0x0f);
        let g = pwm_to_rgb(ch[i * 3 + 1] & 0x0f);
        let b = pwm_to_rgb(ch[i * 3 + 2] & 0x0f);
        colors[i] = (r, g, b);
    }
    colors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_23_dpi_points_roundtrip() {
        for &dpi in &DPI_TABLE {
            let code = dpi_to_code(dpi).expect("valid dpi");
            let decoded = code_to_dpi(code).expect("valid code");
            assert_eq!(dpi, decoded);
        }
    }

    #[test]
    fn test_invalid_dpi() {
        assert!(dpi_to_code(8600).is_err());
        assert!(dpi_to_code(0).is_err());
        assert!(dpi_to_code(2200).is_err());
        assert!(dpi_to_code(12000).is_err());
        assert!(dpi_to_code(16000).is_err());
    }

    #[test]
    fn test_base_mode_golden_encoding() {
        let base_slots = [200, 400, 800, 1600, 3200, 6400];
        let plans = encode_dpi_slots(&base_slots).unwrap();
        assert_eq!(plans.len(), 8);
        assert_eq!(plans[0].bank, 0);
        assert_eq!(plans[0].addr, 0x0010);
        assert_eq!(plans[0].data, 0x0700);

        assert_eq!(plans[1].bank, 1);
        assert_eq!(plans[1].addr, 0x0010);
        assert_eq!(plans[1].data, 0x0700);

        assert_eq!(plans[2].bank, 2);
        assert_eq!(plans[2].addr, 0x0010);
        assert_eq!(plans[2].data, 0x0700);

        assert_eq!(plans[3].bank, 3);
        assert_eq!(plans[3].addr, 0x0010);
        assert_eq!(plans[3].data, 0x0720); // Bank 3 = 0x20

        assert_eq!(plans[4].bank, 4);
        assert_eq!(plans[4].addr, 0x0010);
        assert_eq!(plans[4].data, 0x0708); // Bank 4 = 0x08

        assert_eq!(plans[5].bank, 5);
        assert_eq!(plans[5].addr, 0x0010);
        assert_eq!(plans[5].data, 0x0712); // Bank 5 = 0x12

        assert_eq!(plans[6].bank, 6);
        assert_eq!(plans[6].addr, 0x0010);
        assert_eq!(plans[6].data, 0x0748); // Bank 6 = 0x48

        assert_eq!(plans[7].bank, 7);
        assert_eq!(plans[7].addr, 0x0010);
        assert_eq!(plans[7].data, 0x07d1); // Bank 7 = 0xd1

        let decoded = decode_dpi_slots(0x20, 0x12, 0x48, 0xd1).unwrap();
        assert_eq!(decoded, base_slots);
    }

    #[test]
    fn test_hi1_mode_golden_encoding() {
        let hi1_slots = [8000, 10400, 11200, 8800, 12800, 9600];
        let plans = encode_dpi_slots(&hi1_slots).unwrap();
        assert_eq!(plans.len(), 8);
        assert_eq!(plans[3].data, 0x073f);
        assert_eq!(plans[5].data, 0x0736);
        assert_eq!(plans[6].data, 0x0774);
        assert_eq!(plans[7].data, 0x0785);

        let decoded = decode_dpi_slots(0x3f, 0x36, 0x74, 0x85).unwrap();
        assert_eq!(decoded, hi1_slots);
    }

    #[test]
    fn test_hi2_mode_golden_encoding() {
        let hi2_slots = [8800, 9600, 9600, 10400, 11200, 12800];
        let plans = encode_dpi_slots(&hi2_slots).unwrap();
        assert_eq!(plans.len(), 8);
        assert_eq!(plans[3].data, 0x073f);
        assert_eq!(plans[5].data, 0x0745);
        assert_eq!(plans[6].data, 0x0756);
        assert_eq!(plans[7].data, 0x0778);

        let decoded = decode_dpi_slots(0x3f, 0x45, 0x56, 0x78).unwrap();
        assert_eq!(decoded, hi2_slots);
    }

    #[test]
    fn test_dpi_block_full_8_banks_live_golden() {
        let slots = [200, 800, 200, 800, 200, 800];
        let plans = encode_dpi_slots(&slots).unwrap();
        assert_eq!(plans.len(), 8);

        let expected_bytes = [0x00, 0x00, 0x00, 0x00, 0x08, 0x14, 0x14, 0x14];
        for bank in 0..8usize {
            assert_eq!(plans[bank].bank, bank as u8);
            assert_eq!(plans[bank].addr, 0x0010);
            assert_eq!(plans[bank].data, 0x0700 | (expected_bytes[bank] as u16));
        }
    }

    #[test]
    fn test_default_slot_colors_encoding() {
        let plans = encode_slot_colors(&DEFAULT_SLOT_COLORS, &DEFAULT_SECOND_COLORS, 8).unwrap();
        assert_eq!(plans.len(), 32);
        // Bank 0..2 @ 0x0020 is Slot 1 Yellow (255, 255, 0) -> PWM (0, 0, 15) -> bytes 0x00, 0x10, 0xff
        assert_eq!(plans[0].data, 0x0700);
        assert_eq!(plans[1].data, 0x0710);
        assert_eq!(plans[2].data, 0x07ff);
        // Bank 3..5 @ 0x0020 is Slot 2 Blue (0, 0, 255) -> PWM (15, 15, 0) -> bytes 0xff, 0x8f, 0x00
        assert_eq!(plans[3].data, 0x07ff);
        assert_eq!(plans[4].data, 0x078f);
        assert_eq!(plans[5].data, 0x0700);
    }

    #[test]
    fn test_all_brightness_levels_roundtrip() {
        // Wire contract frozen: 0x0700 | ((8-level)<<4) for every level 0..=8.
        for level in 0..=8u8 {
            let wire = brightness_to_bank7(level).expect("valid level");
            assert_eq!(wire, 0x0700 | (((8 - level) as u16) << 4));
            assert_eq!(bank7_to_brightness(wire).expect("valid nibble"), level);
        }
        assert_eq!(brightness_to_bank7(0).unwrap(), 0x0780);
        assert_eq!(brightness_to_bank7(1).unwrap(), 0x0770);
        assert_eq!(brightness_to_bank7(8).unwrap(), 0x0700);
    }

    #[test]
    fn test_normalize_brightness_level() {
        // Nibble 8 (level 0) is outside the OEM domain 0..7 and lights as 100%.
        assert_eq!(normalize_brightness_level(0), 8);
        for level in 1..=8u8 {
            assert_eq!(
                normalize_brightness_level(level),
                level,
                "level {level} must be identity"
            );
        }
        // Idempotent: re-normalizing never moves the value again.
        for level in 0..=8u8 {
            let once = normalize_brightness_level(level);
            assert_eq!(normalize_brightness_level(once), once);
        }
    }

    #[test]
    fn test_brightness_percent_table() {
        let expected = [
            (0u8, 100.0f64),
            (1, 0.0),
            (2, 25.0),
            (3, 37.5),
            (4, 50.0),
            (5, 62.5),
            (6, 75.0),
            (7, 87.5),
            (8, 100.0),
        ];
        for (level, percent) in expected {
            assert_eq!(
                brightness_percent(level),
                percent,
                "level {level} must render as {percent}%"
            );
        }
        // Level 1 is the real «off»; level 0 is only 100% under another name.
        assert_eq!(brightness_percent(1), 0.0);
        assert_eq!(brightness_percent(0), brightness_percent(8));
    }

    #[test]
    fn test_encode_color_block_38() {
        // Levels 25% (2 -> 0x0760), 50% (4 -> 0x0740), 100% (8 -> 0x0700)
        let plans100 = encode_color_block_38(8).expect("100% brightness should succeed");
        assert_eq!(plans100.len(), 8);
        assert_eq!(plans100[0].addr, 0x0038);
        assert_eq!(plans100[0].data, 0x0700);
        assert_eq!(plans100[1].data, 0x0700);
        assert_eq!(plans100[2].data, 0x070a);
        assert_eq!(plans100[3].data, 0x0700);
        assert_eq!(plans100[4].data, 0x0700);
        assert_eq!(plans100[5].data, 0x0700);
        assert_eq!(plans100[6].data, 0x078f);
        assert_eq!(plans100[7].data, 0x0700);

        let plans50 = encode_color_block_38(4).expect("50% brightness should succeed");
        assert_eq!(plans50[7].data, 0x0740);

        let plans25 = encode_color_block_38(2).expect("25% brightness should succeed");
        assert_eq!(plans25[7].data, 0x0760);

        assert!(encode_color_block_38(9).is_err());
    }

    #[test]
    fn test_encode_color_session_rb_golden() {
        let main_rb: [(u8, u8, u8); 6] = [
            (255, 0, 0),    // Slot 0: Red (PWM: 0, 15, 15)
            (17, 238, 238), // Slot 1: Blue / trace teal (PWM: 14, 1, 1)
            (255, 0, 0),    // Slot 2: Red
            (17, 238, 238), // Slot 3: Blue / trace teal
            (255, 0, 0),    // Slot 4: Red
            (17, 238, 238), // Slot 5: Blue / trace teal
        ];
        let second_rb = DEFAULT_SECOND_COLORS;

        let plans = encode_color_session(&main_rb, &second_rb, 8).unwrap();
        assert_eq!(plans.len(), 32);

        // Ground truth bytes from a live USB capture (R-B-R-B-R-B scenario)
        let expected_0x20: [u8; 8] = [0x00, 0x1f, 0xff, 0xfe, 0x81, 0x01, 0x00, 0xff];
        let expected_0x28: [u8; 8] = [0x8f, 0xfe, 0x01, 0xb1, 0x00, 0xff, 0xff, 0xfe];
        let expected_0x30: [u8; 8] = [0x01, 0x01, 0xff, 0x7f, 0x00, 0x00, 0x00, 0x00];
        let expected_0x38: [u8; 8] = [0x00, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x8f, 0x00];

        for bank in 0..8usize {
            assert_eq!(plans[bank].bank, bank as u8);
            assert_eq!(plans[bank].addr, 0x0020);
            assert_eq!(plans[bank].data, 0x0700 | (expected_0x20[bank] as u16));

            assert_eq!(plans[8 + bank].bank, bank as u8);
            assert_eq!(plans[8 + bank].addr, 0x0028);
            assert_eq!(plans[8 + bank].data, 0x0700 | (expected_0x28[bank] as u16));

            assert_eq!(plans[16 + bank].bank, bank as u8);
            assert_eq!(plans[16 + bank].addr, 0x0030);
            assert_eq!(plans[16 + bank].data, 0x0700 | (expected_0x30[bank] as u16));

            assert_eq!(plans[24 + bank].bank, bank as u8);
            assert_eq!(plans[24 + bank].addr, 0x0038);
            assert_eq!(plans[24 + bank].data, 0x0700 | (expected_0x38[bank] as u16));
        }
    }
}
