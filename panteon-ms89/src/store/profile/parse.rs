use crate::model::{ButtonAction, ButtonBinding, DpiSlot, PollingRate, Profile, DEFAULT_SECOND_COLORS};
use crate::store::{parse_key_value, strip_quotes};
use crate::PanteonError;

enum Section {
    Root,
    DpiTable,
    FireTable,
    LedTable,
    DpiLevel,
    ButtonBinding,
}

/// Parse TOML string into Profile supporting both legacy and expanded sections
pub fn parse(toml_str: &str) -> Result<Profile, PanteonError> {
    let mut id: u8 = 0;
    let mut name = String::new();
    let mut active_dpi_index: u8 = 1;
    let mut polling_rate_hz: u16 = 1000;
    let mut fire_delay_ms: u16 = 50;
    let mut led_effect = "Амбилайт".to_string();
    let mut led_speed: u8 = 3;
    let mut led_brightness: u8 = 4;
    let mut led_all_colors: bool = true;

    let mut dpi_table_slots: Vec<u16> = Vec::new();
    let mut dpi_table_colors: Vec<[u8; 3]> = Vec::new();

    let mut dpi_levels = Vec::new();
    let mut current_dpi: Option<DpiSlotBuilder> = None;

    let mut button_bindings = Vec::new();
    let mut current_btn: Option<ButtonBindingBuilder> = None;

    let mut current_section = Section::Root;

    for line in toml_str.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line == "[dpi]" {
            flush_builders(&mut current_dpi, &mut dpi_levels, &mut current_btn, &mut button_bindings)?;
            current_section = Section::DpiTable;
            continue;
        }
        if line == "[fire]" {
            flush_builders(&mut current_dpi, &mut dpi_levels, &mut current_btn, &mut button_bindings)?;
            current_section = Section::FireTable;
            continue;
        }
        if line == "[led]" {
            flush_builders(&mut current_dpi, &mut dpi_levels, &mut current_btn, &mut button_bindings)?;
            current_section = Section::LedTable;
            continue;
        }
        if line == "[[dpi_levels]]" {
            flush_builders(&mut current_dpi, &mut dpi_levels, &mut current_btn, &mut button_bindings)?;
            current_section = Section::DpiLevel;
            current_dpi = Some(DpiSlotBuilder::default());
            continue;
        }
        if line == "[[button_bindings]]" {
            flush_builders(&mut current_dpi, &mut dpi_levels, &mut current_btn, &mut button_bindings)?;
            current_section = Section::ButtonBinding;
            current_btn = Some(ButtonBindingBuilder::default());
            continue;
        }

        let (key, val) = parse_key_value(line)?;
        match current_section {
            Section::Root => match key {
                "id" => id = val.parse::<u8>().map_err(|_| PanteonError::Protocol("Invalid profile id"))?,
                "name" => name = strip_quotes(val).to_string(),
                "active_dpi_index" => active_dpi_index = val.parse::<u8>().map_err(|_| PanteonError::Protocol("Invalid active_dpi_index"))?,
                "polling_rate" => polling_rate_hz = val.parse::<u16>().map_err(|_| PanteonError::Protocol("Invalid polling_rate"))?,
                _ => {}
            },
            Section::DpiTable => match key {
                "slots" => dpi_table_slots = parse_u16_array(val)?,
                "active" => active_dpi_index = val.parse::<u8>().map_err(|_| PanteonError::Protocol("Invalid active"))?,
                "colors" => dpi_table_colors = parse_hex_colors_array(val)?,
                _ => {}
            },
            Section::FireTable => {
                if key == "delay_ms" {
                    fire_delay_ms = val.parse::<u16>().map_err(|_| PanteonError::Protocol("Invalid delay_ms"))?;
                }
            }
            Section::LedTable => match key {
                "effect" => led_effect = strip_quotes(val).to_string(),
                "speed" => led_speed = val.parse::<u8>().map_err(|_| PanteonError::Protocol("Invalid speed"))?,
                "brightness" => led_brightness = val.parse::<u8>().map_err(|_| PanteonError::Protocol("Invalid brightness"))?,
                "all_colors" => led_all_colors = val.parse::<bool>().map_err(|_| PanteonError::Protocol("Invalid all_colors"))?,
                _ => {}
            },
            Section::DpiLevel => {
                if let Some(ref mut b) = current_dpi {
                    match key {
                        "level_index" => b.level_index = Some(val.parse::<u8>().map_err(|_| PanteonError::Protocol("Invalid level_index"))?),
                        "dpi" => b.dpi = Some(val.parse::<u16>().map_err(|_| PanteonError::Protocol("Invalid dpi"))?),
                        "enabled" => b.enabled = Some(val.parse::<bool>().map_err(|_| PanteonError::Protocol("Invalid enabled"))?),
                        "color" | "color_rgb" | "main" | "main_color" => b.main = Some(parse_rgb_array(val)?),
                        "second" | "second_color" => b.second = Some(parse_rgb_array(val)?),
                        _ => {}
                    }
                }
            }
            Section::ButtonBinding => {
                if let Some(ref mut b) = current_btn {
                    match key {
                        "button_index" => b.button_index = Some(val.parse::<u8>().map_err(|_| PanteonError::Protocol("Invalid button_index"))?),
                        "action" => b.action = Some(ButtonAction::from_str_checked(strip_quotes(val))?),
                        _ => {}
                    }
                }
            }
        }
    }

    flush_builders(&mut current_dpi, &mut dpi_levels, &mut current_btn, &mut button_bindings)?;

    if dpi_levels.is_empty() && !dpi_table_slots.is_empty() {
        for (idx, &slot_dpi) in dpi_table_slots.iter().enumerate() {
            let color = dpi_table_colors.get(idx).copied().unwrap_or([255, 255, 0]);
            let second = DEFAULT_SECOND_COLORS.get(idx).copied().unwrap_or(color);
            dpi_levels.push(DpiSlot {
                level_index: (idx + 1) as u8,
                dpi: slot_dpi,
                enabled: true,
                main: color,
                second,
            });
        }
    }

    let polling_rate = PollingRate::from_hz(polling_rate_hz)
        .ok_or(PanteonError::Protocol("Unsupported polling rate Hz"))?;

    Ok(Profile {
        id,
        name,
        active_dpi_index,
        polling_rate,
        dpi_levels,
        button_bindings,
        fire_delay_ms,
        led_effect,
        led_speed,
        led_brightness,
        led_all_colors,
    })
}

fn flush_builders(
    current_dpi: &mut Option<DpiSlotBuilder>,
    dpi_levels: &mut Vec<DpiSlot>,
    current_btn: &mut Option<ButtonBindingBuilder>,
    button_bindings: &mut Vec<ButtonBinding>,
) -> Result<(), PanteonError> {
    if let Some(builder) = current_dpi.take() {
        dpi_levels.push(builder.build()?);
    }
    if let Some(builder) = current_btn.take() {
        button_bindings.push(builder.build()?);
    }
    Ok(())
}

/// Alias for parse
pub fn parse_toml(toml_str: &str) -> Result<Profile, PanteonError> {
    parse(toml_str)
}

#[derive(Default)]
struct DpiSlotBuilder {
    level_index: Option<u8>,
    dpi: Option<u16>,
    enabled: Option<bool>,
    main: Option<[u8; 3]>,
    second: Option<[u8; 3]>,
}

impl DpiSlotBuilder {
    fn build(self) -> Result<DpiSlot, PanteonError> {
        let level_index = self.level_index.ok_or(PanteonError::ProtocolDetailed {
            context: "DpiLevel",
            detail: "missing required field 'level_index'".to_string(),
        })?;
        let dpi = self.dpi.ok_or(PanteonError::ProtocolDetailed {
            context: "DpiLevel",
            detail: "missing required field 'dpi'".to_string(),
        })?;
        let enabled = self.enabled.ok_or(PanteonError::ProtocolDetailed {
            context: "DpiLevel",
            detail: "missing required field 'enabled'".to_string(),
        })?;
        let main = self.main.ok_or(PanteonError::ProtocolDetailed {
            context: "DpiLevel",
            detail: "missing required field 'color'".to_string(),
        })?;
        let second = self.second.unwrap_or_else(|| {
            let idx = level_index.saturating_sub(1) as usize;
            if idx < 6 {
                DEFAULT_SECOND_COLORS[idx]
            } else {
                main
            }
        });
        Ok(DpiSlot {
            level_index,
            dpi,
            enabled,
            main,
            second,
        })
    }
}

#[derive(Default)]
struct ButtonBindingBuilder {
    button_index: Option<u8>,
    action: Option<ButtonAction>,
}

impl ButtonBindingBuilder {
    fn build(self) -> Result<ButtonBinding, PanteonError> {
        let button_index = self.button_index.ok_or(PanteonError::ProtocolDetailed {
            context: "ButtonBinding",
            detail: "missing required field 'button_index'".to_string(),
        })?;
        let action = self.action.ok_or(PanteonError::ProtocolDetailed {
            context: "ButtonBinding",
            detail: "missing required field 'action'".to_string(),
        })?;
        Ok(ButtonBinding {
            button_index,
            action,
        })
    }
}

fn parse_rgb_array(val: &str) -> Result<[u8; 3], PanteonError> {
    let trimmed = val.trim().trim_start_matches('[').trim_end_matches(']');
    let parts: Vec<&str> = trimmed.split(',').map(|s| s.trim()).collect();
    if parts.len() != 3 {
        return Err(PanteonError::Protocol("RGB array must contain exactly 3 elements"));
    }
    let r = parts[0].parse::<u8>().map_err(|_| PanteonError::Protocol("Invalid red byte"))?;
    let g = parts[1].parse::<u8>().map_err(|_| PanteonError::Protocol("Invalid green byte"))?;
    let b = parts[2].parse::<u8>().map_err(|_| PanteonError::Protocol("Invalid blue byte"))?;
    Ok([r, g, b])
}

fn parse_u16_array(val: &str) -> Result<Vec<u16>, PanteonError> {
    let trimmed = val.trim().trim_start_matches('[').trim_end_matches(']');
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    trimmed.split(',').map(|s| s.trim().parse::<u16>().map_err(|_| PanteonError::Protocol("Invalid integer in array"))).collect()
}

fn parse_hex_colors_array(val: &str) -> Result<Vec<[u8; 3]>, PanteonError> {
    let trimmed = val.trim().trim_start_matches('[').trim_end_matches(']');
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let mut colors = Vec::new();
    for part in trimmed.split(',') {
        let clean = strip_quotes(part.trim()).trim_start_matches('#');
        if clean.len() == 6 {
            let r = u8::from_str_radix(&clean[0..2], 16).map_err(|_| PanteonError::Protocol("Invalid hex color"))?;
            let g = u8::from_str_radix(&clean[2..4], 16).map_err(|_| PanteonError::Protocol("Invalid hex color"))?;
            let b = u8::from_str_radix(&clean[4..6], 16).map_err(|_| PanteonError::Protocol("Invalid hex color"))?;
            colors.push([r, g, b]);
        }
    }
    Ok(colors)
}
