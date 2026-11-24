//! Register map addresses (bank, addr) from USB trace analysis.

/// Active DPI slot selection register: Bank 0, Addr 0x0018.
/// Confirmed across all 6 slots in sessions B, H, I, J, K, L:
/// - Slot 1: 0x0718 (confirmed by ctrl_b)
/// - Slot 2: 0x0719 (confirmed by ctrl_h)
/// - Slot 3: 0x071a (confirmed by ctrl_a, ctrl_i)
/// - Slot 4: 0x071b (confirmed by ctrl_j)
/// - Slot 5: 0x071c (confirmed by ctrl_k)
/// - Slot 6: 0x071d (confirmed by ctrl_l)
///
/// Value formula: 0x0700 | (0x18 + slot - 1), slot in 1..=6.
pub const ACTIVE_DPI_SLOT: (u8, u16) = (0x00, 0x0018);

/// Alias for backward compatibility with previous hypothesis name
pub const CONFIG_STATUS: (u8, u16) = ACTIVE_DPI_SLOT;

/// Polling rate register: Bank 6, Addr 0x0018.
/// Confirmed across all 4 measured points:
/// - 1000 Hz: 0x0700 (confirmed by ctrl_f)
/// - 500 Hz:  0x0701 (confirmed by ctrl_a, ctrl_f)
/// - 250 Hz:  0x0703 (confirmed by ctrl_u_polling_500-250, median 3.9999 ms)
/// - 125 Hz:  0x0707 (confirmed by ctrl_m)
pub const POLLING_RATE: (u8, u16) = (0x06, 0x0018);

/// LED configuration/effect register: Bank 4, Addr 0x0018.
/// Confirmed by ctrl_e_led: 0x07f3 -> 0x07d3 (bit 0x20 toggled).
/// Full effect/color mapping is hypothesis, unconfirmed.
pub const LED_EFFECT: (u8, u16) = (0x04, 0x0018);

/// LED configuration/effect and speed register: Bank 4, Addr 0x0018.
/// - High nibble: effect (0x8 = DPI-Breath, 0x9 = RGB Breath / Off, 0xd = Neon, 0xf = Default).
/// - Low nibble: speed (0x0 = 25%, 0x2 = 50%, 0x5 = 100%, 0x7 = LED Off).
pub const LED_EFFECT_SPEED: (u8, u16) = (0x04, 0x0018);

/// LED color palette bitmask register: Bank 5, Addr 0x0018.
/// - 0xff: all colors
/// - 0x80 | (1 << c): custom color mask (c in 0..=6: c=0 Yellow, c=3 Green, c=6 Purple).
pub const LED_COLOR_PALETTE: (u8, u16) = (0x05, 0x0018);

/// Global LED brightness register: Bank 7, Addr 0x0038 (R8).
/// Active-low encoding: 0x0700 | (((8 - level) as u16) << 4), level 0..=8.
/// - Level 8 (100%): 0x0700 (confirmed by wire traces bright_3_steps_to-*.pcap)
/// - Level 4 (50%):  0x0740 (confirmed by wire traces bright_3_steps_to-*.pcap)
/// - Level 2 (25%):  0x0760 (confirmed by wire traces bright_3_steps_to-*.pcap)
/// - Level 6 (75%):  0x0720 (confidence-gated)
/// - Level 0 (0%):   0x0780 (confidence-gated)
pub const LED_BRIGHTNESS: (u8, u16) = (0x07, 0x0038);

/// Profile selector register: Bank 6, Addr 0x0070.
/// Value: 0x0700 | profile_index (0..=3).
pub const ACTIVE_PROFILE: (u8, u16) = (0x06, 0x0070);

/// Button rebind register: Bank 7, Addr 0x0008.
pub const BUTTON_REBIND: (u8, u16) = (0x07, 0x0008);

/// DPI slot enable mask / configuration register: Bank 5, Addr 0x0010.
/// Confirmed across sessions B, C, H..M:
/// - 0x0712 in ctrl_a_open
/// - 0x0722 in ctrl_b and ctrl_c
/// - 0x0724 in ctrl_h..ctrl_m
pub const DPI_CONFIG: (u8, u16) = (0x05, 0x0010);

/// General profile status register: Bank 6, Addr 0x0010.
/// Observed in ctrl_d, ctrl_e, ctrl_f, ctrl_i..ctrl_m: 0x075c / 0x074c / 0x078c / 0x078d.
/// Hypothesis, unconfirmed: profile update status/counter.
pub const PROFILE_STATUS: (u8, u16) = (0x06, 0x0010);

/// Sensor parameter register for high DPI levels: Bank 3, Addr 0x0010.
/// Observed in ctrl_k (0x0720) and ctrl_l (0x0700) vs default 0x0730.
/// Hypothesis, unconfirmed: hardware sensor prescaler.
pub const DPI_PARAM_B3: (u8, u16) = (0x03, 0x0010);

/// Sensor divider register for high DPI levels: Bank 7, Addr 0x0010.
/// Observed in ctrl_k (0x07e8) and ctrl_l (0x07ef) vs default 0x0728.
/// Hypothesis, unconfirmed: high DPI multiplier/divider.
pub const DPI_PARAM_B7: (u8, u16) = (0x07, 0x0010);

/// Fire button delay low byte register: Bank 2, Addr 0x0018.
/// Holds (delay_ms & 0xff). Range 8..=300 ms. Confirmed in fire_sp_to-8ms / fire_sp_to-300.
pub const FIRE_DELAY_LOW: (u8, u16) = (0x02, 0x0018);

/// Fire button delay high byte register: Bank 3, Addr 0x0018.
/// Holds ((delay_ms >> 8) & 0xff). Range 8..=300 ms. Confirmed in fire_sp_to-8ms / fire_sp_to-300.
pub const FIRE_DELAY_HIGH: (u8, u16) = (0x03, 0x0018);

/// Fire delay primary address alias (Bank 2, Addr 0x0018).
pub const FIRE_DELAY: (u8, u16) = FIRE_DELAY_LOW;

/// Polling rate register data values (Little-Endian u16)
pub mod polling_rate_codes {
    /// 1000 Hz polling rate (period 1 ms) — Confirmed
    pub const HZ_1000: u16 = 0x0700;
    /// 500 Hz polling rate (period 2 ms) — Confirmed
    pub const HZ_500: u16 = 0x0701;
    /// 250 Hz polling rate (period 4 ms) — Confirmed: OEM trace ctrl_u_polling_500-250.pcap frame 1995 (bank6=0x0703), physical verification median 3.9999 ms (live USB capture)
    pub const HZ_250: u16 = 0x0703;
    /// 125 Hz polling rate (period 8 ms) — Confirmed
    pub const HZ_125: u16 = 0x0707;
}

/// DPI slot codes for Bank 0, Addr 0x0018
pub mod dpi_slot_codes {
    pub const SLOT_1: u16 = 0x0718; // Confirmed (ctrl_b)
    pub const SLOT_2: u16 = 0x0719; // Confirmed (ctrl_h)
    pub const SLOT_3: u16 = 0x071a; // Confirmed (ctrl_a, ctrl_i)
    pub const SLOT_4: u16 = 0x071b; // Confirmed (ctrl_j)
    pub const SLOT_5: u16 = 0x071c; // Confirmed (ctrl_k)
    pub const SLOT_6: u16 = 0x071d; // Confirmed (ctrl_l)
}
