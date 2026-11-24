//! LED wire-кодек: Bank 4 Addr 0x0018 (эффект + скорость) и Bank 5 Addr 0x0018
//! (маска палитры).
//!
//! Единственный источник правды о LED wire-байтах (AGENTS §4): энкодеры и декодеры
//! одной схемы живут вместе. GUI хранит только презентационные label-строки.
//!
//! Данные режимов подтверждены живыми USB-трейсами (таблица 14 эффектов)
//! и перенесены 1:1 из прежней GUI-таблицы — новых значений не вводится.

/// Wire-спека режима LED: всё, кроме презентационного label.
///
/// `mode_index` — семантический индекс режима `0..=13` (не обязательно позиция
/// в GUI-каталоге: режимы 9..12 разделяют базовые байты с режимами 1..4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedModeSpec {
    pub mode_index: u8,
    pub base_byte: u8,
    pub default_speed: u8,
    pub supports_speed: bool,
    pub supports_brightness: bool,
    pub supports_palette: bool,
}

/// 14 LED-режимов в порядке OEM-утилиты (индекс = порядок dropdown).
pub static LED_MODE_SPECS: [LedModeSpec; 14] = [
    LedModeSpec {
        mode_index: 0,
        base_byte: 0x80,
        default_speed: 3,
        supports_speed: true,
        supports_brightness: false,
        supports_palette: false,
    },
    LedModeSpec {
        mode_index: 1,
        base_byte: 0x90,
        default_speed: 0,
        supports_speed: true,
        supports_brightness: false,
        supports_palette: true,
    },
    LedModeSpec {
        mode_index: 2,
        base_byte: 0xa0,
        default_speed: 0,
        supports_speed: false,
        supports_brightness: true,
        supports_palette: false,
    },
    LedModeSpec {
        mode_index: 3,
        base_byte: 0xb0,
        default_speed: 0,
        supports_speed: true,
        supports_brightness: true,
        supports_palette: true,
    },
    LedModeSpec {
        mode_index: 4,
        base_byte: 0xc0,
        default_speed: 0,
        supports_speed: true,
        supports_brightness: true,
        supports_palette: true,
    },
    LedModeSpec {
        mode_index: 5,
        base_byte: 0xd0,
        default_speed: 3,
        supports_speed: true,
        supports_brightness: true,
        supports_palette: true,
    },
    LedModeSpec {
        mode_index: 6,
        base_byte: 0xe0,
        default_speed: 0,
        supports_speed: true,
        supports_brightness: false,
        supports_palette: false,
    },
    LedModeSpec {
        mode_index: 7,
        base_byte: 0xf0,
        default_speed: 3,
        supports_speed: true,
        supports_brightness: false,
        supports_palette: false,
    },
    LedModeSpec {
        mode_index: 8,
        base_byte: 0x80,
        default_speed: 0,
        supports_speed: true,
        supports_brightness: false,
        supports_palette: false,
    },
    LedModeSpec {
        mode_index: 9,
        base_byte: 0x90,
        default_speed: 0,
        supports_speed: false,
        supports_brightness: false,
        supports_palette: false,
    },
    LedModeSpec {
        mode_index: 10,
        base_byte: 0xa0,
        default_speed: 0,
        supports_speed: true,
        supports_brightness: false,
        supports_palette: false,
    },
    LedModeSpec {
        mode_index: 11,
        base_byte: 0xb0,
        default_speed: 0,
        supports_speed: true,
        supports_brightness: false,
        supports_palette: false,
    },
    LedModeSpec {
        mode_index: 12,
        base_byte: 0x98,
        default_speed: 0,
        supports_speed: false,
        supports_brightness: false,
        supports_palette: false,
    },
    LedModeSpec {
        mode_index: 13,
        base_byte: 0x97,
        default_speed: 7,
        supports_speed: false,
        supports_brightness: false,
        supports_palette: false,
    },
];

impl LedModeSpec {
    /// Calculate Bank 4 Addr 0x0018 data_lo byte for given speed.
    pub fn wire_byte(&self, speed: u8) -> u8 {
        if self.mode_index == 13 {
            0x97 // LED Off
        } else if self.mode_index == 12 {
            0x98 // Button Response
        } else if self.supports_speed {
            self.base_byte | (speed & 0x07)
        } else {
            self.base_byte
        }
    }
}

/// Inverse of [`LedModeSpec::wire_byte`]: register low byte -> (mode_index, speed).
///
/// Several modes share a base byte (0/8 -> 0x80, 1/9 -> 0x90, 2/10 -> 0xa0,
/// 3/11 -> 0xb0): the wire byte cannot distinguish them, so the lowest mode
/// index wins. Modes 12/13 ignore the speed field and are matched exactly.
pub fn decode_effect_byte(byte: u8) -> Option<(u8, u8)> {
    if byte == 0x97 {
        return Some((13, LED_MODE_SPECS[13].default_speed));
    }
    if byte == 0x98 {
        return Some((12, LED_MODE_SPECS[12].default_speed));
    }
    LED_MODE_SPECS
        .iter()
        .filter(|spec| spec.mode_index != 12 && spec.mode_index != 13)
        .find_map(|spec| {
            (0..=7u8)
                .find(|speed| spec.wire_byte(*speed) == byte)
                .map(|speed| (spec.mode_index, speed))
        })
}

/// Compute LED color palette bitmask.
///
/// Rules:
/// - If `all_colors` is true: returns 0xff (all 7 colors enabled).
/// - Otherwise: returns `0x80 | \sum (1 << c)` for each selected color `c` in 0..=6.
/// - If no colors are selected in custom mode, returns 0x80 (custom with none active).
pub fn encode_palette_mask(all_colors: bool, selected: &[bool; 7]) -> u8 {
    if all_colors {
        return 0xff;
    }
    let mut mask = 0x80u8;
    for (i, &sel) in selected.iter().enumerate() {
        if sel {
            mask |= 1 << (i as u8);
        }
    }
    mask
}

/// Inverse of [`encode_palette_mask`]: register low byte -> (all_colors, bits).
/// `0xff` is the canonical "all colours" mask; otherwise bit 7 marks a custom mask.
pub fn decode_palette_mask(mask: u8) -> (bool, [bool; 7]) {
    if mask == 0xff {
        return (true, [false; 7]);
    }
    let mut selected = [false; 7];
    for (i, bit) in selected.iter_mut().enumerate() {
        *bit = (mask & (1 << i)) != 0;
    }
    (false, selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------
    // Характеризационные дампы, снятые с БАЗОВОЙ реализации (5c743e5) до
    // рефакторинга: временный тест печатал `gui::LED_EFFECT_OPTIONS[i]
    // .wire_byte(s)` для i∈0..14, s∈0..=7 и `gui::palette_from_mask(m)` /
    // `gui::led_option_from_wire_byte(b)` для всех 256 значений. Таблицы ниже
    // перенесены из этого дампа дословно; временный дамп-тест не коммитился.
    // ---------------------------------------------------------------------

    /// base `wire_byte` для 14 режимов × скорости 0..=7.
    const EXPECTED_WIRE_MATRIX: [[u8; 8]; 14] = [
        [0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87],
        [0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97],
        [0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0xa0],
        [0xb0, 0xb1, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7],
        [0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7],
        [0xd0, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7],
        [0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7],
        [0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7],
        [0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87],
        [0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90],
        [0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7],
        [0xb0, 0xb1, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7],
        [0x98, 0x98, 0x98, 0x98, 0x98, 0x98, 0x98, 0x98],
        [0x97, 0x97, 0x97, 0x97, 0x97, 0x97, 0x97, 0x97],
    ];

    /// base `palette_from_mask(m).0` для m∈0..=255.
    const EXPECTED_PALETTE_ALL: [bool; 256] = [
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false,
        false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, true,
    ];

    /// base `palette_from_mask(m).1`, сжатый в битовую маску бит 0..6.
    const EXPECTED_PALETTE_BITS: [u8; 256] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f,
        0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
        0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f,
        0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f,
        0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e, 0x6f,
        0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x7b, 0x7c, 0x7d, 0x7e, 0x7f,
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f,
        0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
        0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f,
        0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f,
        0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e, 0x6f,
        0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x7b, 0x7c, 0x7d, 0x7e, 0x00,
    ];

    /// base ре-энкод `encode_palette_mask(decode_palette_mask(m))` для m∈0..=255.
    const EXPECTED_PALETTE_REENCODE: [u8; 256] = [
        0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x8b, 0x8c, 0x8d, 0x8e, 0x8f,
        0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0x9b, 0x9c, 0x9d, 0x9e, 0x9f,
        0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae, 0xaf,
        0xb0, 0xb1, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xbb, 0xbc, 0xbd, 0xbe, 0xbf,
        0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xcb, 0xcc, 0xcd, 0xce, 0xcf,
        0xd0, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xdb, 0xdc, 0xdd, 0xde, 0xdf,
        0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xeb, 0xec, 0xed, 0xee, 0xef,
        0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa, 0xfb, 0xfc, 0xfd, 0xfe, 0xff,
        0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x8b, 0x8c, 0x8d, 0x8e, 0x8f,
        0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0x9b, 0x9c, 0x9d, 0x9e, 0x9f,
        0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae, 0xaf,
        0xb0, 0xb1, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xbb, 0xbc, 0xbd, 0xbe, 0xbf,
        0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xcb, 0xcc, 0xcd, 0xce, 0xcf,
        0xd0, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xdb, 0xdc, 0xdd, 0xde, 0xdf,
        0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xeb, 0xec, 0xed, 0xee, 0xef,
        0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa, 0xfb, 0xfc, 0xfd, 0xfe, 0xff,
    ];

    /// base `led_option_from_wire_byte(b)` для b∈0..=255 (семантический mode_index).
    const EXPECTED_DECODE: [Option<(u8, u8)>; 256] = [
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
        Some((0, 0)), Some((0, 1)), Some((0, 2)), Some((0, 3)), Some((0, 4)), Some((0, 5)), Some((0, 6)), Some((0, 7)),
        None, None, None, None, None, None, None, None,
        Some((1, 0)), Some((1, 1)), Some((1, 2)), Some((1, 3)), Some((1, 4)), Some((1, 5)), Some((1, 6)), Some((13, 7)),
        Some((12, 0)), None, None, None, None, None, None, None,
        Some((2, 0)), Some((10, 1)), Some((10, 2)), Some((10, 3)), Some((10, 4)), Some((10, 5)), Some((10, 6)), Some((10, 7)),
        None, None, None, None, None, None, None, None,
        Some((3, 0)), Some((3, 1)), Some((3, 2)), Some((3, 3)), Some((3, 4)), Some((3, 5)), Some((3, 6)), Some((3, 7)),
        None, None, None, None, None, None, None, None,
        Some((4, 0)), Some((4, 1)), Some((4, 2)), Some((4, 3)), Some((4, 4)), Some((4, 5)), Some((4, 6)), Some((4, 7)),
        None, None, None, None, None, None, None, None,
        Some((5, 0)), Some((5, 1)), Some((5, 2)), Some((5, 3)), Some((5, 4)), Some((5, 5)), Some((5, 6)), Some((5, 7)),
        None, None, None, None, None, None, None, None,
        Some((6, 0)), Some((6, 1)), Some((6, 2)), Some((6, 3)), Some((6, 4)), Some((6, 5)), Some((6, 6)), Some((6, 7)),
        None, None, None, None, None, None, None, None,
        Some((7, 0)), Some((7, 1)), Some((7, 2)), Some((7, 3)), Some((7, 4)), Some((7, 5)), Some((7, 6)), Some((7, 7)),
        None, None, None, None, None, None, None, None,
    ];

    #[test]
    fn test_spec_table_covers_modes_0_to_13() {
        assert_eq!(LED_MODE_SPECS.len(), 14);
        let mut seen = [false; 14];
        for spec in LED_MODE_SPECS.iter() {
            assert!(spec.mode_index < 14, "mode_index out of range: {}", spec.mode_index);
            assert!(!seen[spec.mode_index as usize], "duplicate mode_index {}", spec.mode_index);
            seen[spec.mode_index as usize] = true;
        }
        assert!(seen.iter().all(|s| *s), "every mode_index 0..=13 must be present");
    }

    #[test]
    fn test_wire_matrix_matches_base_characterization() {
        for (i, spec) in LED_MODE_SPECS.iter().enumerate() {
            for speed in 0..=7u8 {
                assert_eq!(
                    spec.wire_byte(speed),
                    EXPECTED_WIRE_MATRIX[i][speed as usize],
                    "mode {i} (mode_index {}) speed {speed}",
                    spec.mode_index
                );
            }
        }
    }

    #[test]
    fn test_led_palette_mask_confirmed_points() {
        // 1. All colors -> 0xff
        assert_eq!(encode_palette_mask(true, &[false; 7]), 0xff);
        assert_eq!(encode_palette_mask(true, &[true; 7]), 0xff);

        // 2. Yellow only (c=0) -> 0x81
        let mut yellow = [false; 7];
        yellow[0] = true;
        assert_eq!(encode_palette_mask(false, &yellow), 0x81);

        // 3. Green only (c=3) -> 0x88
        let mut green = [false; 7];
        green[3] = true;
        assert_eq!(encode_palette_mask(false, &green), 0x88);

        // 4. Purple only (c=6) -> 0xc0
        let mut purple = [false; 7];
        purple[6] = true;
        assert_eq!(encode_palette_mask(false, &purple), 0xc0);

        // 5. Purple + Green (c=3, 6) -> 0xc8
        let mut purple_green = [false; 7];
        purple_green[3] = true;
        purple_green[6] = true;
        assert_eq!(encode_palette_mask(false, &purple_green), 0xc8);
    }

    #[test]
    fn test_led_palette_mask_combinatorics() {
        // Test all single colors
        for i in 0..7 {
            let mut sel = [false; 7];
            sel[i] = true;
            let expected = 0x80 | (1 << i);
            assert_eq!(encode_palette_mask(false, &sel), expected);
        }

        // Test all 7 colors checked individually in custom mode -> 0x80 | 0x7f = 0xff
        assert_eq!(encode_palette_mask(false, &[true; 7]), 0xff);

        // Test no colors checked in custom mode -> 0x80
        assert_eq!(encode_palette_mask(false, &[false; 7]), 0x80);
    }

    #[test]
    fn test_effect_dropdown_byte_mapping_confirmed() {
        // Ground truth verification against live USB captures:
        // 1. DPI дыхание: 0x80 | speed -> ctrl_n speed 2 is 0x82, speed 3 is 0x83
        assert_eq!(LED_MODE_SPECS[0].wire_byte(2), 0x82);
        assert_eq!(LED_MODE_SPECS[0].wire_byte(3), 0x83);
        assert!(LED_MODE_SPECS[0].supports_speed);
        assert!(!LED_MODE_SPECS[0].supports_brightness);
        assert!(!LED_MODE_SPECS[0].supports_palette);

        // 2. Дыхание «цикл»: 0x90 | speed -> ctrl_o speed 0 (0x90), 2 (0x92), 5 (0x95)
        assert_eq!(LED_MODE_SPECS[1].wire_byte(0), 0x90);
        assert_eq!(LED_MODE_SPECS[1].wire_byte(2), 0x92);
        assert_eq!(LED_MODE_SPECS[1].wire_byte(5), 0x95);
        assert!(LED_MODE_SPECS[1].supports_speed);
        assert!(LED_MODE_SPECS[1].supports_palette);

        // 3. Постоянно: 0xa0, static bright
        assert_eq!(LED_MODE_SPECS[2].wire_byte(0), 0xa0);
        assert!(!LED_MODE_SPECS[2].supports_speed);
        assert!(LED_MODE_SPECS[2].supports_brightness);

        // 4. Течение: 0xb0 -> gui_open.pcapng is 0xb0 (speed 0)
        assert_eq!(LED_MODE_SPECS[3].wire_byte(0), 0xb0);
        assert!(LED_MODE_SPECS[3].supports_speed);
        assert!(LED_MODE_SPECS[3].supports_brightness);
        assert!(LED_MODE_SPECS[3].supports_palette);

        // 6. Комета: 0xd0 | speed -> ctrl_e_led is 0xd3 (speed 3)
        assert_eq!(LED_MODE_SPECS[5].wire_byte(3), 0xd3);

        // 8. Амбилайт: 0xf0 | speed -> ctrl_d_reset & ctrl_t_profile_reset is 0xf3 (speed 3)
        assert_eq!(LED_MODE_SPECS[7].wire_byte(3), 0xf3);

        // 13. Отклик: 0x98
        assert_eq!(LED_MODE_SPECS[12].wire_byte(0), 0x98);

        // 14. Выключено: 0x97 -> ctrl_p_led_off is 0x97
        assert_eq!(LED_MODE_SPECS[13].wire_byte(0), 0x97);
        assert_eq!(LED_MODE_SPECS[13].wire_byte(5), 0x97);
    }

    #[test]
    fn test_led_option_from_wire_byte() {
        // Real capture: bank 4 @ 0x0018 = 0x07d2 -> mode 5 «Комета», speed 2.
        assert_eq!(decode_effect_byte(0xd2), Some((5, 2)));
        // Speed-only variants.
        assert_eq!(decode_effect_byte(0x83), Some((0, 3)));
        assert_eq!(decode_effect_byte(0x90), Some((1, 0)));
        assert_eq!(decode_effect_byte(0xa0), Some((2, 0)));
        // 0x97 / 0x98 are fixed bytes of modes 14 / 13 (speed field ignored).
        assert_eq!(decode_effect_byte(0x97), Some((13, 7)));
        assert_eq!(decode_effect_byte(0x98), Some((12, 0)));
        // Unknown encodings are reported, not guessed.
        assert_eq!(decode_effect_byte(0x00), None);
        assert_eq!(decode_effect_byte(0x70), None);
    }

    #[test]
    fn test_decode_effect_byte_matches_base_characterization() {
        for byte in 0..=255u8 {
            assert_eq!(
                decode_effect_byte(byte),
                EXPECTED_DECODE[byte as usize],
                "byte 0x{byte:02x}"
            );
        }
    }

    #[test]
    fn test_effect_decode_round_trip_is_total() {
        // Every produced byte decodes back to a mode whose re-encode is byte-identical.
        for (i, spec) in LED_MODE_SPECS.iter().enumerate() {
            for speed in 0..=7u8 {
                let byte = spec.wire_byte(speed);
                let back = decode_effect_byte(byte).expect("byte must be recognised");
                let back_spec = LED_MODE_SPECS
                    .iter()
                    .find(|s| s.mode_index == back.0)
                    .unwrap_or_else(|| panic!("decoded mode_index {} is not in the table", back.0));
                assert_eq!(
                    back_spec.wire_byte(back.1),
                    byte,
                    "spec {i} speed {speed} -> 0x{byte:02x} -> {back:?}"
                );
            }
        }
    }

    #[test]
    fn test_palette_from_mask() {
        assert_eq!(decode_palette_mask(0xff), (true, [false; 7]));

        let (all, bits) = decode_palette_mask(0xc8); // Green (bit 3) + Purple (bit 6)
        assert!(!all);
        assert_eq!(bits, [false, false, false, true, false, false, true]);

        let (all, bits) = decode_palette_mask(0x81); // Yellow only
        assert!(!all);
        assert!(bits[0] && !bits.iter().skip(1).any(|b| *b));

        // Canonical masks round-trip through encode_palette_mask.
        for mask in [0xffu8, 0x80, 0x81, 0x88, 0xc0, 0xc8, 0xff] {
            let (all, bits) = decode_palette_mask(mask);
            assert_eq!(encode_palette_mask(all, &bits), mask);
        }
    }

    #[test]
    fn test_palette_mask_all_256_matches_base_characterization() {
        for mask in 0..=255u8 {
            let (all, bits) = decode_palette_mask(mask);
            assert_eq!(all, EXPECTED_PALETTE_ALL[mask as usize], "all flag for 0x{mask:02x}");
            let packed = bits
                .iter()
                .enumerate()
                .fold(0u8, |acc, (i, b)| if *b { acc | (1 << i) } else { acc });
            assert_eq!(packed, EXPECTED_PALETTE_BITS[mask as usize], "bits for 0x{mask:02x}");

            let re = encode_palette_mask(all, &bits);
            assert_eq!(re, EXPECTED_PALETTE_REENCODE[mask as usize], "re-encode for 0x{mask:02x}");

            // Current (base) behavior, deliberately preserved: custom masks below 0x80
            // gain the custom flag bit on re-encode.
            if mask >= 0x80 {
                assert_eq!(re, mask, "identity re-encode expected for 0x{mask:02x}");
            } else {
                assert_eq!(re, mask | 0x80, "re-encode must set custom flag for 0x{mask:02x}");
            }
        }
    }
}