use crate::model::registers;
use crate::PanteonError;

/// Polling rate (USB report rate in Hz)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollingRate {
    Hz125,
    Hz250,
    Hz500,
    Hz1000,
}

impl PollingRate {
    pub fn as_hz(self) -> u16 {
        match self {
            Self::Hz125 => 125,
            Self::Hz250 => 250,
            Self::Hz500 => 500,
            Self::Hz1000 => 1000,
        }
    }

    pub fn from_hz(hz: u16) -> Option<Self> {
        match hz {
            125 => Some(Self::Hz125),
            250 => Some(Self::Hz250),
            500 => Some(Self::Hz500),
            1000 => Some(Self::Hz1000),
            _ => None,
        }
    }

    /// Convert to confirmed register code for Bank 6, Addr 0x0018
    /// (confirmed for all four rates (125/250/500/1000)).
    pub fn to_register_code(self) -> u16 {
        match self {
            Self::Hz1000 => registers::polling_rate_codes::HZ_1000,
            Self::Hz500 => registers::polling_rate_codes::HZ_500,
            Self::Hz250 => registers::polling_rate_codes::HZ_250,
            Self::Hz125 => registers::polling_rate_codes::HZ_125,
        }
    }

    /// Parse from confirmed register code for Bank 6, Addr 0x0018
    /// (confirmed for all four rates (125/250/500/1000)).
    pub fn from_register_code(code: u16) -> Option<Self> {
        match code {
            registers::polling_rate_codes::HZ_1000 => Some(Self::Hz1000),
            registers::polling_rate_codes::HZ_500 => Some(Self::Hz500),
            registers::polling_rate_codes::HZ_250 => Some(Self::Hz250),
            registers::polling_rate_codes::HZ_125 => Some(Self::Hz125),
            _ => None,
        }
    }
}

/// Mouse button action
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ButtonAction {
    LeftClick,
    RightClick,
    MiddleClick,
    Forward,
    Back,
    DpiUp,
    DpiDown,
    Fire,
    LedSwitch,
    Disabled,
    Custom(String),
}

impl ButtonAction {
    pub fn as_str(&self) -> &str {
        match self {
            Self::LeftClick => "LeftClick",
            Self::RightClick => "RightClick",
            Self::MiddleClick => "MiddleClick",
            Self::Forward => "Forward",
            Self::Back => "Back",
            Self::DpiUp => "DpiUp",
            Self::DpiDown => "DpiDown",
            Self::Fire => "Fire",
            Self::LedSwitch => "LedSwitch",
            Self::Disabled => "Disabled",
            Self::Custom(s) => s.as_str(),
        }
    }

    /// Parse button action from string with strict validation (rejects quiet fallback to Custom)
    pub fn from_str_checked(s: &str) -> Result<Self, PanteonError> {
        match s {
            "LeftClick" => Ok(Self::LeftClick),
            "RightClick" => Ok(Self::RightClick),
            "MiddleClick" => Ok(Self::MiddleClick),
            "Forward" => Ok(Self::Forward),
            "Back" => Ok(Self::Back),
            "DpiUp" => Ok(Self::DpiUp),
            "DpiDown" => Ok(Self::DpiDown),
            "Fire" => Ok(Self::Fire),
            "LedSwitch" => Ok(Self::LedSwitch),
            "Disabled" => Ok(Self::Disabled),
            _ => Err(PanteonError::ProtocolDetailed {
                context: "ButtonAction",
                detail: format!("unknown button action '{}'", s),
            }),
        }
    }
}

/// Mouse button binding (buttons 1 through 8)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonBinding {
    pub button_index: u8,
    pub action: ButtonAction,
}

/// Default secondary RGB colors for the 6 DPI slots (corresponding to OEM high-nibble state, ADR-003).
pub const DEFAULT_SECOND_COLORS: [[u8; 3]; 6] = [
    [255, 238, 0],
    [0, 119, 255],
    [255, 0, 119],
    [0, 255, 68],
    [255, 0, 0],
    [0, 255, 255],
];

/// Single DPI slot configuration with two-state color channels (ADR-003).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpiSlot {
    pub level_index: u8,
    pub dpi: u16,
    pub enabled: bool,
    pub main: [u8; 3],
    pub second: [u8; 3],
}

impl DpiSlot {
    pub fn new(level_index: u8, dpi: u16, enabled: bool, main: [u8; 3], second: [u8; 3]) -> Self {
        Self {
            level_index,
            dpi,
            enabled,
            main,
            second,
        }
    }

    /// Calculate 4-bit PWM nibbles for main and second color channels
    /// and pack them into 3 wire bytes: `[R, G, B]`.
    /// Each byte is composed as `(pwm(second) << 4) | (pwm(main) & 0x0f)`.
    pub fn to_pwm_bytes(&self) -> [u8; 3] {
        let r_m = (255 - self.main[0]) >> 4;
        let r_s = (255 - self.second[0]) >> 4;
        let g_m = (255 - self.main[1]) >> 4;
        let g_s = (255 - self.second[1]) >> 4;
        let b_m = (255 - self.main[2]) >> 4;
        let b_s = (255 - self.second[2]) >> 4;

        [
            (r_s << 4) | (r_m & 0x0f),
            (g_s << 4) | (g_m & 0x0f),
            (b_s << 4) | (b_m & 0x0f),
        ]
    }
}

/// Deprecated alias for migration to DpiSlot.
#[deprecated(since = "0.3.0", note = "Use DpiSlot instead")]
pub type DpiLevel = DpiSlot;

/// Configuration profile (Config0 through Config3)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: u8,
    pub name: String,
    pub active_dpi_index: u8,
    pub polling_rate: PollingRate,
    pub dpi_levels: Vec<DpiSlot>,
    pub button_bindings: Vec<ButtonBinding>,
    pub fire_delay_ms: u16,
    pub led_effect: String,
    pub led_speed: u8,
    pub led_brightness: u8,
    pub led_all_colors: bool,
}

impl Profile {
    /// Factory default profile reconstructed from Windows OEM driver defaults
    pub fn factory_default() -> Self {
        Self {
            id: 0,
            name: "Factory Default".to_string(),
            active_dpi_index: 1,
            polling_rate: PollingRate::Hz1000,
            dpi_levels: vec![
                DpiSlot { level_index: 1, dpi: 200, enabled: true, main: [255, 255, 0], second: [255, 238, 0] },
                DpiSlot { level_index: 2, dpi: 400, enabled: true, main: [0, 0, 255], second: [0, 119, 255] },
                DpiSlot { level_index: 3, dpi: 800, enabled: true, main: [255, 0, 255], second: [255, 0, 119] },
                DpiSlot { level_index: 4, dpi: 1600, enabled: true, main: [0, 255, 0], second: [0, 255, 68] },
                DpiSlot { level_index: 5, dpi: 3200, enabled: true, main: [255, 0, 0], second: [255, 0, 0] },
                DpiSlot { level_index: 6, dpi: 6400, enabled: true, main: [0, 255, 255], second: [0, 255, 255] },
            ],
            button_bindings: vec![
                ButtonBinding { button_index: 1, action: ButtonAction::LeftClick },
                ButtonBinding { button_index: 2, action: ButtonAction::RightClick },
                ButtonBinding { button_index: 3, action: ButtonAction::MiddleClick },
                ButtonBinding { button_index: 4, action: ButtonAction::Forward },
                ButtonBinding { button_index: 5, action: ButtonAction::Back },
                ButtonBinding { button_index: 6, action: ButtonAction::DpiUp },
                ButtonBinding { button_index: 7, action: ButtonAction::DpiDown },
                ButtonBinding { button_index: 8, action: ButtonAction::Fire },
            ],
            fire_delay_ms: 50,
            led_effect: "Амбилайт".to_string(),
            led_speed: 3,
            led_brightness: 4,
            led_all_colors: true,
        }
    }

    /// Competitive gaming preset tuned for low latency and high precision
    pub fn high_dpi_competitive() -> Self {
        Self {
            id: 1,
            name: "High-DPI Competitive".to_string(),
            active_dpi_index: 1,
            polling_rate: PollingRate::Hz1000,
            dpi_levels: vec![
                DpiSlot { level_index: 1, dpi: 8000, enabled: true, main: [255, 255, 0], second: [255, 238, 0] },
                DpiSlot { level_index: 2, dpi: 10400, enabled: true, main: [0, 0, 255], second: [0, 119, 255] },
                DpiSlot { level_index: 3, dpi: 11200, enabled: true, main: [255, 0, 255], second: [255, 0, 119] },
                DpiSlot { level_index: 4, dpi: 8800, enabled: true, main: [0, 255, 0], second: [0, 255, 68] },
                DpiSlot { level_index: 5, dpi: 12800, enabled: true, main: [255, 0, 0], second: [255, 0, 0] },
                DpiSlot { level_index: 6, dpi: 9600, enabled: true, main: [0, 255, 255], second: [0, 255, 255] },
            ],
            button_bindings: vec![
                ButtonBinding { button_index: 1, action: ButtonAction::LeftClick },
                ButtonBinding { button_index: 2, action: ButtonAction::RightClick },
                ButtonBinding { button_index: 3, action: ButtonAction::MiddleClick },
                ButtonBinding { button_index: 4, action: ButtonAction::Forward },
                ButtonBinding { button_index: 5, action: ButtonAction::Back },
                ButtonBinding { button_index: 6, action: ButtonAction::DpiUp },
                ButtonBinding { button_index: 7, action: ButtonAction::DpiDown },
                ButtonBinding { button_index: 8, action: ButtonAction::Fire },
            ],
            fire_delay_ms: 8,
            led_effect: "Выключено".to_string(),
            led_speed: 7,
            led_brightness: 0,
            led_all_colors: true,
        }
    }
}

impl Default for Profile {
    fn default() -> Self {
        Self::factory_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpi_slot_to_pwm_bytes() {
        let slot1 = DpiSlot::new(1, 200, true, [255, 255, 0], [255, 238, 0]);
        // R: main 255 -> 0, second 255 -> 0 => 0x00
        // G: main 255 -> 0, second 238 -> 1 => 0x10
        // B: main 0 -> 15, second 0 -> 15 => 0xff
        assert_eq!(slot1.to_pwm_bytes(), [0x00, 0x10, 0xff]);
    }

    #[test]
    fn test_polling_rate_roundtrip_all_four_rates() {
        let rates = [
            (PollingRate::Hz125, 125, registers::polling_rate_codes::HZ_125),
            (PollingRate::Hz250, 250, registers::polling_rate_codes::HZ_250),
            (PollingRate::Hz500, 500, registers::polling_rate_codes::HZ_500),
            (PollingRate::Hz1000, 1000, registers::polling_rate_codes::HZ_1000),
        ];

        for (rate, hz, code) in rates {
            assert_eq!(rate.as_hz(), hz);
            assert_eq!(PollingRate::from_hz(hz), Some(rate));
            assert_eq!(rate.to_register_code(), code);
            assert_eq!(PollingRate::from_register_code(code), Some(rate));
        }

        assert_eq!(PollingRate::from_hz(0), None);
        assert_eq!(PollingRate::from_hz(333), None);
        assert_eq!(PollingRate::from_register_code(0x07ff), None);
        assert_eq!(PollingRate::from_register_code(0x0000), None);
    }
}
