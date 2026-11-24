use eframe::egui;

use crate::codec::{DEFAULT_DPI_SLOTS, DEFAULT_SLOT_COLORS, DPI_TABLE};
use crate::gui::app::PanteonApp;
use crate::gui::check_write_permission;

/// Discrete color presets that the mouse hardware LED can reliably display.
pub const COLOR_PRESETS: [(&str, [u8; 3]); 8] = [
    ("Красный", [255, 0, 0]),
    ("Жёлтый", [255, 255, 0]),
    ("Зелёный", [0, 255, 0]),
    ("Голубой", [0, 255, 255]),
    ("Синий", [0, 0, 255]),
    ("Пурпурный", [255, 0, 255]),
    ("Белый", [255, 255, 255]),
    ("Оранжевый", [255, 128, 0]),
];

/// Width/height of the colour swatch button in the table (Apple-ish small control).
const SWATCH_W: f32 = 28.0;
const SWATCH_H: f32 = 18.0;
/// Palette popup: 8 presets as 2 rows x 4 columns of 20x20 chips.
const CHIP: f32 = 20.0;
const CHIP_GAP: f32 = 8.0;

fn table_header(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(12.0)
            .strong()
            .color(crate::gui::theme::TEXT_SECONDARY),
    );
}

/// Renders the DPI Slots Editor panel (M4.0 / M4.2).
pub fn render_dpi_panel(ui: &mut egui::Ui, app: &mut PanteonApp) {
    egui::Grid::new("dpi_slots_grid")
        .num_columns(4)
        .spacing([16.0, 6.0])
        .striped(true)
        .min_col_width(60.0)
        .show(ui, |ui| {
            table_header(ui, "Активен");
            table_header(ui, "Слот");
            table_header(ui, "Значение DPI");
            table_header(ui, "Цвет подсветки");
            ui.end_row();

            for slot_idx in 0..6 {
                let slot_num = (slot_idx + 1) as u8;
                let selected = app.selected_dpi_slot == slot_num;

                // Column 1: text-less radio, vertically centred on the control row.
                let mut radio_clicked = false;
                ui.allocate_ui_with_layout(
                    egui::vec2(SWATCH_W, 30.0),
                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                    |ui| {
                        let resp = ui.add(egui::RadioButton::new(selected, ""));
                        radio_clicked =
                            crate::gui::theme::tooltip(resp, "Сделать слот активным").clicked();
                    },
                );
                if radio_clicked {
                    app.selected_dpi_slot = slot_num;
                }

                // Column 2: Slot label
                ui.label(format!("Слот {}", slot_num));

                // Column 3: ComboBox for 23 discrete DPI steps
                let current_dpi = app.dpi_slots[slot_idx];
                egui::ComboBox::from_id_salt(format!("dpi_combo_{}", slot_num))
                    .selected_text(format!("{} DPI", current_dpi))
                    .show_ui(ui, |ui| {
                        for &val in &DPI_TABLE {
                            ui.selectable_value(
                                &mut app.dpi_slots[slot_idx],
                                val,
                                format!("{} DPI", val),
                            );
                        }
                    });

                // Column 4: swatch triggers an anchored popup palette (no extra
                // grid column, so the table keeps its 4-column rhythm).
                let palette_open = app.dpi_palette_slot == Some(slot_idx);
                let popup_id = ui.make_persistent_id(("dpi_palette", slot_idx));
                let cur = app.slot_colors[slot_idx];
                let col = egui::Color32::from_rgb(cur[0], cur[1], cur[2]);
                let swatch = egui::Button::new("")
                    .fill(col)
                    .stroke(if palette_open {
                        egui::Stroke::new(2.0, crate::gui::theme::ACCENT)
                    } else {
                        egui::Stroke::new(1.0, crate::gui::theme::CARD_STROKE)
                    })
                    .min_size(egui::vec2(SWATCH_W, SWATCH_H))
                    .corner_radius(egui::CornerRadius::same(5));
                let palette_tip = if palette_open {
                    "Скрыть палитру"
                } else {
                    "Выбрать цвет из палитры"
                };
                let resp = crate::gui::theme::tooltip(ui.add(swatch), palette_tip);
                if resp.clicked() {
                    if palette_open {
                        app.dpi_palette_slot = None;
                        ui.memory_mut(|m| m.close_popup());
                    } else {
                        app.dpi_palette_slot = Some(slot_idx);
                        ui.memory_mut(|m| m.open_popup(popup_id));
                    }
                }

                if palette_open {
                    egui::popup::popup_below_widget(
                        ui,
                        popup_id,
                        &resp,
                        egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            let grid_w = 4.0 * CHIP + 3.0 * CHIP_GAP;
                            ui.set_min_width(grid_w);
                            ui.set_max_width(grid_w);
                            ui.scope(|ui| {
                                ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
                                ui.spacing_mut().item_spacing = egui::vec2(CHIP_GAP, CHIP_GAP);
                                for row in COLOR_PRESETS.chunks(4) {
                                    ui.horizontal(|ui| {
                                        for &(name, rgb) in row {
                                            let chip_col =
                                                egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                                            let is_current = app.slot_colors[slot_idx] == rgb;
                                            let chip = egui::Button::new("")
                                                .small()
                                                .fill(chip_col)
                                                .stroke(if is_current {
                                                    egui::Stroke::new(
                                                        2.0,
                                                        crate::gui::theme::ACCENT,
                                                    )
                                                } else {
                                                    egui::Stroke::NONE
                                                })
                                                .min_size(egui::vec2(CHIP, CHIP))
                                                .corner_radius(egui::CornerRadius::same(5));
                                            if crate::gui::theme::tooltip(ui.add(chip), name)
                                                .clicked()
                                            {
                                                app.slot_colors[slot_idx] = rgb;
                                                app.dpi_palette_slot = None;
                                                ui.memory_mut(|m| m.close_popup());
                                            }
                                        }
                                    });
                                }
                            });
                        },
                    );
                    if !ui.memory(|m| m.is_popup_open(popup_id)) {
                        // Closed by click-outside or Escape.
                        app.dpi_palette_slot = None;
                    }
                }

                ui.end_row();
            }
        });

    ui.add_space(8.0);

    // Reset actions with consistent button heights in a single row
    ui.horizontal(|ui| {
        let reset_colors = ui.button("Сбросить цвета");
        if crate::gui::theme::tooltip(
            reset_colors,
            "Жёлтый, Синий, Пурпурный, Зелёный, Красный, Голубой — заводская палитра",
        )
        .clicked()
        {
            for (i, def) in DEFAULT_SLOT_COLORS.iter().enumerate() {
                app.slot_colors[i] = [def.0, def.1, def.2];
            }
        }

        let reset_dpi = ui.button("Сбросить DPI");
        if crate::gui::theme::tooltip(
            reset_dpi,
            "200, 400, 800, 1600, 3200, 6400 DPI и заводская палитра цветов",
        )
        .clicked()
        {
            app.dpi_slots = DEFAULT_DPI_SLOTS;
            for (i, def) in DEFAULT_SLOT_COLORS.iter().enumerate() {
                app.slot_colors[i] = [def.0, def.1, def.2];
            }
        }
    });

    let can_w = check_write_permission(app.current_identity()).is_ok();
    if !can_w {
        ui.label(
            egui::RichText::new("Запись заблокирована: устройство не верифицировано (запустите verify-protocol в CLI)")
                .small()
                .weak(),
        );
    }
}
