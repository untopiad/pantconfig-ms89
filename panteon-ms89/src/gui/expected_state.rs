use eframe::egui;

use crate::gui::app::PanteonApp;
use crate::gui::theme;
use crate::model::{registers, DeviceState, ShadowEntry};

/// Interpret POLLING_RATE register value (Bank 6, Addr 0x0018)
pub fn interpret_polling_rate(val: u16) -> String {
    match val {
        0x0700 => "1000 Гц".to_string(),
        0x0701 => "500 Гц".to_string(),
        0x0703 => "250 Гц".to_string(),
        0x0707 => "125 Гц".to_string(),
        other => format!("0x{:04x} (нестандартное)", other),
    }
}

/// Interpret ACTIVE_DPI_SLOT register value (Bank 0, Addr 0x0018)
pub fn interpret_dpi_slot(val: u16) -> String {
    match DeviceState::raw_to_slot_index(val) {
        Ok(slot) => format!("Слот {}", slot),
        Err(_) => format!("0x{:04x} (слот не распознан)", val),
    }
}

/// Interpret LED_EFFECT_SPEED register value (Bank 4, Addr 0x0018)
pub fn interpret_led_effect_speed(val: u16) -> String {
    let byte = (val & 0xff) as u8;
    let effect = byte >> 4;
    let speed = byte & 0x0f;

    let effect_str = match effect {
        0x8 => "Дыхание DPI",
        0x9 => {
            if speed == 7 {
                return "Подсветка выключена".to_string();
            }
            "Дыхание RGB"
        }
        0xd => "Неон",
        0xf => "По умолчанию",
        other => return format!("Эффект 0x{:x}, скорость {}", other, speed),
    };

    let speed_str = match speed {
        0 => "25% (мин)",
        1 => "~35%",
        2 => "50%",
        3 => "~70%",
        4 => "~85%",
        5 => "100% (макс)",
        7 => "выкл",
        other => return format!("{}, скорость {}", effect_str, other),
    };

    format!("{}, {}", effect_str, speed_str)
}

/// Interpret LED_COLOR_PALETTE register value (Bank 5, Addr 0x0018)
pub fn interpret_color_palette(val: u16) -> String {
    let mask = (val & 0xff) as u8;
    if mask == 0xff {
        "Все цвета".to_string()
    } else if (mask & 0x80) != 0 {
        let names = [
            "Жёлтый",
            "Оранжевый",
            "Красный",
            "Зелёный",
            "Голубой",
            "Синий",
            "Пурпурный",
        ];
        let mut active = Vec::new();
        for (i, name) in names.iter().enumerate() {
            if (mask & (1 << i)) != 0 {
                active.push(*name);
            }
        }
        if active.is_empty() {
            format!("0x{:02x} (пустой набор)", mask)
        } else {
            active.join(", ")
        }
    } else {
        format!("0x{:02x} (сырая маска)", mask)
    }
}

/// Interpret ACTIVE_PROFILE register value (Bank 6, Addr 0x0070)
pub fn interpret_profile(val: u16) -> String {
    let idx = (val & 0x03) as u8;
    format!("Профиль {}", idx + 1)
}

/// Interpret FIRE_DELAY register values (Bank 2 and Bank 3 @ 0x0018)
pub fn interpret_fire_delay(low_val: Option<u16>, high_val: Option<u16>) -> String {
    match (low_val, high_val) {
        (Some(lo), Some(hi)) => {
            let delay = ((hi & 0xff) << 8) | (lo & 0xff);
            format!("{} мс", delay)
        }
        (Some(lo), None) => format!("{} мс (только младший байт)", lo & 0xff),
        _ => "По умолчанию (50 мс)".to_string(),
    }
}

struct RegisterSummary {
    /// Human-facing row name (no register identifiers on the primary surface).
    name: &'static str,
    bank: u8,
    addr: u16,
    entry: Option<ShadowEntry>,
    interpretation: String,
}

pub fn render_expected_state_panel(ui: &mut egui::Ui, app: &PanteonApp) {
    let now = crate::model::current_unix_timestamp();

    let covered_dpi_banks = {
        let mut banks = std::collections::HashSet::new();
        for e in &app.ledger.entries {
            if e.addr == 0x0010 && e.bank < 8 {
                banks.insert(e.bank);
            }
        }
        banks.len()
    };

    // Gauge row: one compact line (ring + last write + coverage) so the tab fits 900px.
    ui.horizontal(|ui| {
        // Radial indicator gauge for profile DPI coverage
        let (rect, _response) =
            ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::hover());
        let center = rect.center();
        let radius = 12.0;
        let painter = ui.painter();

        // Background track circle
        painter.circle_stroke(
            center,
            radius,
            egui::Stroke::new(5.0, theme::SEPARATOR),
        );

        // Progress arc
        let fraction = (covered_dpi_banks as f32 / 8.0).clamp(0.0, 1.0);
        if fraction > 0.0 {
            let steps = ((32.0 * fraction).ceil() as usize).max(2);
            let points: Vec<egui::Pos2> = (0..=steps)
                .map(|i| {
                    let t = (i as f32 / steps as f32) * fraction;
                    let angle = -std::f32::consts::FRAC_PI_2 + t * std::f32::consts::TAU;
                    center + egui::vec2(angle.cos() * radius, angle.sin() * radius)
                })
                .collect();
            painter.add(egui::Shape::line(
                points,
                egui::Stroke::new(5.0, theme::ACCENT),
            ));
        }

        ui.add_space(8.0);
        if let Some(last_entry) = app.ledger.entries.last() {
            let resp = ui.label(format!(
                "Последняя запись: {}",
                crate::gui::app::format_human_ts(last_entry.written_at, now)
            ));
            // Raw bank/address stay available, just not on the primary surface.
            theme::tooltip(
                resp,
                format!("Банк {} · 0x{:04x} · 0x{:04x}", last_entry.bank, last_entry.addr, last_entry.data),
            );
        } else {
            ui.label(
                egui::RichText::new("Записи в текущей сессии не производились.")
                    .color(theme::TEXT_SECONDARY),
            );
        }
        ui.label(egui::RichText::new("·").color(theme::TEXT_SECONDARY));
        let coverage = if covered_dpi_banks >= 8 {
            "Профиль записан полностью".to_string()
        } else {
            format!("Записано {} из 8 блоков профиля", covered_dpi_banks)
        };
        ui.label(egui::RichText::new(coverage).small().weak());
    });

    ui.add_space(8.0);

    let get_last = |bank: u8, addr: u16| -> Option<ShadowEntry> {
        app.ledger
            .entries
            .iter()
            .rev()
            .find(|e| e.bank == bank && e.addr == addr)
            .cloned()
    };

    let poll_entry = get_last(registers::POLLING_RATE.0, registers::POLLING_RATE.1);
    let poll_interp = poll_entry
        .as_ref()
        .map(|e| interpret_polling_rate(e.data))
        .unwrap_or_else(|| "Не изменялся (по умолчанию 1000 Гц)".to_string());

    let dpi_entry = get_last(registers::ACTIVE_DPI_SLOT.0, registers::ACTIVE_DPI_SLOT.1);
    let dpi_interp = dpi_entry
        .as_ref()
        .map(|e| interpret_dpi_slot(e.data))
        .unwrap_or_else(|| "Не изменялся (по умолчанию Слот 1)".to_string());

    let led_entry = get_last(registers::LED_EFFECT_SPEED.0, registers::LED_EFFECT_SPEED.1);
    let led_interp = led_entry
        .as_ref()
        .map(|e| interpret_led_effect_speed(e.data))
        .unwrap_or_else(|| "Не изменялся (по умолчанию)".to_string());

    let pal_entry = get_last(registers::LED_COLOR_PALETTE.0, registers::LED_COLOR_PALETTE.1);
    let pal_interp = pal_entry
        .as_ref()
        .map(|e| interpret_color_palette(e.data))
        .unwrap_or_else(|| "Не изменялся (по умолчанию все цвета)".to_string());

    let prof_entry = get_last(registers::ACTIVE_PROFILE.0, registers::ACTIVE_PROFILE.1);
    let prof_interp = prof_entry
        .as_ref()
        .map(|e| interpret_profile(e.data))
        .unwrap_or_else(|| "Не изменялся (Профиль 1)".to_string());

    let fire_low_entry = get_last(registers::FIRE_DELAY_LOW.0, registers::FIRE_DELAY_LOW.1);
    let fire_high_entry = get_last(registers::FIRE_DELAY_HIGH.0, registers::FIRE_DELAY_HIGH.1);
    let fire_interp = interpret_fire_delay(
        fire_low_entry.as_ref().map(|e| e.data),
        fire_high_entry.as_ref().map(|e| e.data),
    );

    let items = [
        RegisterSummary {
            name: "Частота опроса",
            bank: registers::POLLING_RATE.0,
            addr: registers::POLLING_RATE.1,
            entry: poll_entry,
            interpretation: poll_interp,
        },
        RegisterSummary {
            name: "Активный слот",
            bank: registers::ACTIVE_DPI_SLOT.0,
            addr: registers::ACTIVE_DPI_SLOT.1,
            entry: dpi_entry,
            interpretation: dpi_interp,
        },
        RegisterSummary {
            name: "Задержка огня",
            bank: registers::FIRE_DELAY_LOW.0,
            addr: registers::FIRE_DELAY_LOW.1,
            entry: fire_low_entry,
            interpretation: fire_interp,
        },
        RegisterSummary {
            name: "Подсветка",
            bank: registers::LED_EFFECT_SPEED.0,
            addr: registers::LED_EFFECT_SPEED.1,
            entry: led_entry,
            interpretation: led_interp,
        },
        RegisterSummary {
            name: "Палитра",
            bank: registers::LED_COLOR_PALETTE.0,
            addr: registers::LED_COLOR_PALETTE.1,
            entry: pal_entry,
            interpretation: pal_interp,
        },
        RegisterSummary {
            name: "Профиль",
            bank: registers::ACTIVE_PROFILE.0,
            addr: registers::ACTIVE_PROFILE.1,
            entry: prof_entry,
            interpretation: prof_interp,
        },
    ];

    let header = |ui: &mut egui::Ui, text: &str| {
        ui.label(
            egui::RichText::new(text)
                .size(12.0)
                .strong()
                .color(theme::TEXT_SECONDARY),
        );
    };

    egui::Grid::new("expected_state_grid")
        .striped(true)
        .min_col_width(90.0)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            header(ui, "Параметр");
            header(ui, "Значение");
            header(ui, "Обновлено");
            ui.end_row();

            for item in &items {
                // Human name on the surface; bank/address/raw value in the tooltip.
                let name = ui.label(item.name);
                let raw = match &item.entry {
                    Some(e) => format!("0x{:04x}", e.data),
                    None => "запись отсутствует".to_string(),
                };
                theme::tooltip(
                    name,
                    format!("Банк {} · 0x{:04x} · {}", item.bank, item.addr, raw),
                );

                if item.entry.is_some() {
                    ui.label(&item.interpretation);
                } else {
                    ui.label(egui::RichText::new(&item.interpretation).color(theme::TEXT_SECONDARY));
                }

                match &item.entry {
                    Some(e) => {
                        ui.label(crate::gui::app::format_human_ts(e.written_at, now));
                    }
                    None => {
                        ui.label(egui::RichText::new("—").color(theme::TEXT_SECONDARY));
                    }
                }
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpretations() {
        assert_eq!(interpret_polling_rate(0x0700), "1000 Гц");
        assert_eq!(interpret_polling_rate(0x0701), "500 Гц");
        assert_eq!(interpret_polling_rate(0x0707), "125 Гц");

        assert_eq!(interpret_dpi_slot(0x0718), "Слот 1");
        assert_eq!(interpret_dpi_slot(0x071a), "Слот 3");
        assert_eq!(interpret_dpi_slot(0x071d), "Слот 6");

        assert_eq!(interpret_led_effect_speed(0x0790), "Дыхание RGB, 25% (мин)");
        assert_eq!(interpret_led_effect_speed(0x0797), "Подсветка выключена");
        assert_eq!(interpret_led_effect_speed(0x0782), "Дыхание DPI, 50%");
        assert_eq!(interpret_led_effect_speed(0x07d5), "Неон, 100% (макс)");

        assert_eq!(interpret_color_palette(0x07ff), "Все цвета");
        // 0x80 | 1<<3 (Green) | 1<<6 (Purple) = 0xc8
        assert_eq!(interpret_color_palette(0x07c8), "Зелёный, Пурпурный");

        assert_eq!(interpret_profile(0x0700), "Профиль 1");
        assert_eq!(interpret_profile(0x0703), "Профиль 4");

        assert_eq!(interpret_fire_delay(Some(0x0708), Some(0x0700)), "8 мс");
        assert_eq!(interpret_fire_delay(Some(0x072c), Some(0x0701)), "300 мс");
        assert_eq!(interpret_fire_delay(Some(0x0732), Some(0x0700)), "50 мс");
        assert_eq!(interpret_fire_delay(None, None), "По умолчанию (50 мс)");
    }
}