use eframe::egui;

use crate::codec::DescriptorPage;
use crate::codec::led::encode_palette_mask;
use crate::device::DiscoveredDevice;
use crate::gui::app::{DryRunSummary, PanteonApp};
use crate::gui::theme;
use crate::gui::{LED_EFFECT_OPTIONS, PALETTE_COLOR_NAMES};
use crate::model::DeviceState;

/// Renders primary CTA button ("Принять") for tabs.
/// Disabled state carries an inline reason (Apple: inline validation, not silent dead buttons).
pub fn render_primary_cta(ui: &mut egui::Ui, enabled: bool) -> egui::Response {
    let btn = egui::Button::new(
        egui::RichText::new("Принять")
            .strong()
            .color(if enabled {
                egui::Color32::WHITE
            } else {
                theme::TEXT_SECONDARY
            }),
    )
    .min_size(egui::vec2(140.0, theme::BTN_H))
    .corner_radius(egui::CornerRadius::same(theme::NAV_RADIUS as u8));
    let btn = if enabled {
        btn.fill(theme::ACCENT)
    } else {
        btn.fill(theme::HOVER_BG)
    };
    let resp = ui.add_enabled(enabled, btn);
    if !enabled {
        theme::disabled_tooltip(resp, "Нет изменений для применения — измените параметры вкладки")
    } else {
        theme::tooltip(resp, "Открыть подтверждение записи")
    }
}

/// Human labels for device actions; the technical meaning lives in the tooltips.
fn device_actions(ui: &mut egui::Ui, app: &mut PanteonApp) {
    if theme::tooltip(ui.button("Обновить"), "Обновить список подключённых устройств").clicked() {
        app.refresh_devices();
    }
    if theme::tooltip(
        ui.button("Переподключить USB"),
        "Мягкий перезапуск USB (перенумерация устройства).\nНужен для применения новой частоты опроса.",
    )
    .clicked()
    {
        app.execute_soft_reset();
    }
}

fn hex_format(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn render_diff_dialog(ctx: &egui::Context, app: &mut PanteonApp) {
    if !app.diff_dialog_open {
        return;
    }

    egui::Window::new("Изменения конфигурации")
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new("Список изменённых параметров относительно текущего состояния в памяти / журнале:")
                    .weak(),
            );
            ui.add_space(6.0);

            let diffs = app.diff_entries();
            if diffs.is_empty() {
                ui.label("Изменений нет.");
            } else {
                egui::Grid::new("diff_entries_grid")
                    .striped(true)
                    .spacing([18.0, 6.0])
                    .show(ui, |ui| {
                        for title in ["Параметр", "Было (в памяти)", "Стало"] {
                            ui.label(
                                egui::RichText::new(title)
                                    .size(12.0)
                                    .strong()
                                    .color(theme::TEXT_SECONDARY),
                            );
                        }
                        ui.end_row();

                        for (param, old_v, new_v) in diffs {
                            ui.label(param);
                            ui.label(old_v);
                            ui.colored_label(theme::ACCENT, new_v);
                            ui.end_row();
                        }
                    });
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if ui.button("Закрыть").clicked() {
                    app.diff_dialog_open = false;
                }
            });
        });
}

pub fn render_dry_run_summary(ui: &mut egui::Ui, summary: &DryRunSummary) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Сводка плана (Dry-run):").strong());
            ui.label(&summary.title);
        });
        ui.label(format!("Что меняется: {}", summary.description));
        ui.label(format!("Число кадров: {}", summary.frame_count));
        if !summary.sample_frame.is_empty() {
            ui.label(format!("Пример кадра: {}", summary.sample_frame));
        }

        egui::CollapsingHeader::new(format!("Кадры USB ({})", summary.frame_count))
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .show(ui, |ui| {
                        for line in &summary.all_frames {
                            ui.monospace(line);
                        }
                    });
            });
    });
}

// ---------------------------------------------------------------------------
// Tab 1: Состояние (Read-only)
// ---------------------------------------------------------------------------

/// Apple-style elevated card container (rounded, subtle 1px stroke, generous padding).
/// All cards span the full content width (no shrink-wrap): the first thing the inner
/// closure does is claim `available_width()`, so every card on every tab aligns to
/// CONTENT_W — or to the available width when the window is narrower than that.
/// Vertical rhythm between cards is added by the tab renderers (`theme::GAP`), not by
/// an outer margin here, so the last card adds no trailing overflow.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(theme::CARD)
        .corner_radius(egui::CornerRadius::same(theme::CARD_R as u8))
        .stroke(egui::Stroke::new(1.0, theme::CARD_STROKE))
        .inner_margin(egui::Margin::same(theme::CARD_PAD as i8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Section title inside a card (small, strong, quiet).
pub fn card_title(ui: &mut egui::Ui, title: &str, subtitle: Option<&str>) {
    ui.label(egui::RichText::new(title).strong().size(15.0).color(theme::TEXT));
    if let Some(sub) = subtitle {
        ui.label(egui::RichText::new(sub).size(12.0).color(theme::TEXT_SECONDARY));
    }
    ui.add_space(8.0);
}

pub fn render_state_tab(ui: &mut egui::Ui, app: &mut PanteonApp) {
    card(ui, |ui| {
        card_title(ui, "Устройство", None);
        render_device_panel(ui, app);
    });
    ui.add_space(theme::GAP);

    card(ui, |ui| {
        card_title(ui, "Состояние оборудования", None);
        render_state_panel(ui, app);
    });
    ui.add_space(theme::GAP);

    card(ui, |ui| {
        card_title(
            ui,
            "Текущая конфигурация",
            Some("По журналу записей: устройство не отдаёт настройки обратно."),
        );
        crate::gui::render_expected_state_panel(ui, app);
    });
}

pub fn render_device_panel(ui: &mut egui::Ui, app: &mut PanteonApp) {
    if app.devices.is_empty() {
        ui.label("Подключённые устройства PANTEON MS89 не найдены. Проверьте подключение USB и правила udev.");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            device_actions(ui, app);
        });
        return;
    }

    let dev_label = |d: &DiscoveredDevice, all: &[DiscoveredDevice]| -> String {
        let multiple_interfaces = all
            .iter()
            .filter(|other| {
                other.vendor_id == d.vendor_id
                    && other.product_id == d.product_id
                    && other.usb_bus == d.usb_bus
                    && other.usb_dev == d.usb_dev
            })
            .count()
            > 1;

        let iface_suffix = if multiple_interfaces {
            if let Some(iface) = d.interface_number {
                format!(" (интерфейс {})", iface)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let verified = if d.identity.protocol_verified {
            "проверен"
        } else {
            "не проверен"
        };

        format!(
            "{}{} (PID 0x{:04x}, {})",
            d.path.display(),
            iface_suffix,
            d.product_id,
            verified
        )
    };

    ui.horizontal_wrapped(|ui| {
        let mut selection_changed = false;
        let mut current_idx = app.selected_device_idx;
        let selected_text = match current_idx {
            Some(idx) => dev_label(&app.devices[idx], &app.devices),
            None => "Выберите интерфейс".to_string(),
        };

        egui::ComboBox::from_id_salt("device_iface_select")
            .width(320.0)
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for (i, d) in app.devices.iter().enumerate() {
                    let label = dev_label(d, &app.devices);
                    if ui.selectable_value(&mut current_idx, Some(i), label).clicked() {
                        selection_changed = true;
                    }
                }
            });

        if selection_changed {
            app.selected_device_idx = current_idx;
            app.query_device_state();
        }

        device_actions(ui, app);
    });
}

pub fn render_state_panel(ui: &mut egui::Ui, app: &PanteonApp) {
    // Two columns only: the third one ("Теневой журнал") duplicated the register table
    // below and needed its own nested ScrollArea, which clipped on narrow windows.
    // Wrapped layout + min column width keeps both readable at 780px.
    ui.horizontal_wrapped(|ui| {
        ui.vertical(|ui| {
            ui.set_min_width(240.0);
            // Dense info block: tighter vertical rhythm than the global tokens.
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(egui::RichText::new("Рукопожатие и флаги").strong());
            if let Some(DeviceState {
                handshake: Some(hs),
                ..
            }) = &app.device_state
            {
                ui.label(format!("Сигнатура: 0x{:04x}", hs.signature));
                ui.label(format!("Ревизия:   0x{:02x}", hs.revision));
                ui.label(format!("Статус:    0x{:02x}", hs.status));
                ui.label(format!("Флаги:     0x{:02x}", hs.flags));
            } else {
                ui.label("Рукопожатие не запрошено");
            }
        });

        ui.add_space(24.0);

        ui.vertical(|ui| {
            ui.set_min_width(240.0);
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(egui::RichText::new("Дескриптор устройства").strong());
            if let Some(DescriptorPage {
                sync,
                unlock_state,
                pid,
                vid_lo,
                write_echo,
                ..
            }) = &app.descriptor_page
            {
                ui.label(format!("Синхронизация: 0x{:02x}, 0x{:02x}", sync.0, sync.1));
                ui.label(format!("Разблокировка: 0x{:02x}", unlock_state));
                ui.label(format!("PID / VID:     0x{:04x} / 0x{:02x}", pid, vid_lo));
                ui.label(format!("Write Echo:    0x{:02x}", write_echo));
            } else {
                ui.label("Дескриптор не запрошен");
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Tab 2: DPI
// ---------------------------------------------------------------------------

pub fn render_dpi_tab(ui: &mut egui::Ui, app: &mut PanteonApp) {
    card(ui, |ui| {
        card_title(ui, "Слоты DPI", None);
        crate::gui::render_dpi_panel(ui, app);
    });
    ui.add_space(theme::GAP);

    card(ui, |ui| {
        card_title(ui, "Частота опроса", None);
        render_polling_panel(ui, app);
    });
    ui.add_space(theme::GAP);

    card(ui, |ui| {
        card_title(ui, "Отдача", None);
        render_fire_delay_panel(ui, app);

        if let Some(summary) = &app.dry_run_summary {
            if summary.title.contains("DPI")
                || summary.title.contains("опроса")
                || summary.title.contains("отдачи")
                || summary.title.contains("Отдача")
            {
                ui.add_space(8.0);
                render_dry_run_summary(ui, summary);
            }
        }
    });
}

pub fn render_polling_panel(ui: &mut egui::Ui, app: &mut PanteonApp) {
    ui.horizontal(|ui| {
        for &rate in &[125u16, 250, 500, 1000] {
            let selected = app.selected_polling == rate;
            let resp = ui
                .scope(|ui| {
                    // Native hover: unselected pills fill with HOVER_BG (Apple "Response"),
                    // selected pill stays accent-filled.
                    let (idle_fill, idle_text) = if selected {
                        (theme::ACCENT, egui::Color32::WHITE)
                    } else {
                        (egui::Color32::TRANSPARENT, theme::TEXT_SECONDARY)
                    };
                    let hover_fill = if selected { theme::ACCENT } else { theme::HOVER_BG };
                    let visuals = ui.visuals_mut();
                    visuals.widgets.inactive.weak_bg_fill = idle_fill;
                    visuals.widgets.inactive.bg_fill = idle_fill;
                    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, idle_text);
                    visuals.widgets.hovered.weak_bg_fill = hover_fill;
                    visuals.widgets.hovered.bg_fill = hover_fill;
                    visuals.widgets.hovered.fg_stroke =
                        egui::Stroke::new(1.0, egui::Color32::WHITE);
                    visuals.widgets.active.weak_bg_fill = hover_fill;
                    visuals.widgets.active.bg_fill = hover_fill;
                    visuals.widgets.active.fg_stroke =
                        egui::Stroke::new(1.0, egui::Color32::WHITE);

                    let btn = egui::Button::new(
                        egui::RichText::new(format!("{} Гц", rate)).size(13.0),
                    )
                    .min_size(egui::vec2(88.0, theme::PILL_H))
                    .corner_radius(egui::CornerRadius::same(10))
                    .stroke(if selected {
                        egui::Stroke::NONE
                    } else {
                        egui::Stroke::new(1.0, theme::CARD_STROKE)
                    });
                    ui.add(btn)
                })
                .inner;

            if theme::tooltip(resp, format!("Частота опроса: {} Гц", rate)).clicked() {
                app.selected_polling = rate;
            }
        }
    });
}

pub fn styled_slider(ui: &mut egui::Ui, slider: egui::Slider) -> egui::Response {
    ui.scope(|ui| {
        let dark_stroke = theme::CARD_STROKE;
        let accent = theme::ACCENT;
        ui.visuals_mut().widgets.inactive.bg_fill = egui::Color32::WHITE;
        ui.visuals_mut().widgets.inactive.fg_stroke = egui::Stroke::new(1.0, dark_stroke);
        ui.visuals_mut().widgets.hovered.bg_fill = egui::Color32::WHITE;
        ui.visuals_mut().widgets.hovered.fg_stroke = egui::Stroke::new(1.5, accent);
        ui.visuals_mut().widgets.active.bg_fill = egui::Color32::WHITE;
        ui.visuals_mut().widgets.active.fg_stroke = egui::Stroke::new(1.5, accent);
        ui.visuals_mut().selection.bg_fill = accent;
        ui.visuals_mut().slider_trailing_fill = true;
        ui.visuals_mut().handle_shape = egui::style::HandleShape::Circle;
        ui.add(slider)
    })
    .inner
}

pub fn styled_slider_enabled(ui: &mut egui::Ui, enabled: bool, slider: egui::Slider) -> egui::Response {
    ui.scope(|ui| {
        let dark_stroke = theme::CARD_STROKE;
        let accent = theme::ACCENT;
        ui.visuals_mut().widgets.inactive.bg_fill = egui::Color32::WHITE;
        ui.visuals_mut().widgets.inactive.fg_stroke = egui::Stroke::new(1.0, dark_stroke);
        ui.visuals_mut().widgets.hovered.bg_fill = egui::Color32::WHITE;
        ui.visuals_mut().widgets.hovered.fg_stroke = egui::Stroke::new(1.5, accent);
        ui.visuals_mut().widgets.active.bg_fill = egui::Color32::WHITE;
        ui.visuals_mut().widgets.active.fg_stroke = egui::Stroke::new(1.5, accent);
        ui.visuals_mut().selection.bg_fill = accent;
        ui.visuals_mut().slider_trailing_fill = true;
        ui.visuals_mut().handle_shape = egui::style::HandleShape::Circle;
        ui.add_enabled(enabled, slider)
    })
    .inner
}

pub fn render_fire_delay_panel(ui: &mut egui::Ui, app: &mut PanteonApp) {
    ui.horizontal(|ui| {
        ui.add_sized([CONTROL_LABEL_W, 20.0], egui::Label::new("Задержка огня"));
        ui.spacing_mut().slider_width = SLIDER_W;
        let resp = styled_slider(
            ui,
            egui::Slider::new(&mut app.fire_delay, 8..=300).suffix(" мс"),
        );
        theme::tooltip(
            resp,
            "Интервал между выстрелами при удержании кнопки огня (8–300 мс)",
        );
    });
}

// ---------------------------------------------------------------------------
// Tab 3: Подсветка
// ---------------------------------------------------------------------------

pub fn render_lighting_tab(ui: &mut egui::Ui, app: &mut PanteonApp) {
    card(ui, |ui| {
        // No card title here: the tab header already says "Подсветка".
        render_led_panel(ui, app);

        if let Some(summary) = &app.dry_run_summary {
            if summary.title.contains("Подсветка") {
                ui.add_space(8.0);
                render_dry_run_summary(ui, summary);
            }
        }
    });
}

/// Fixed width of the control label column so every row's control starts at the same x.
const CONTROL_LABEL_W: f32 = 90.0;
/// Track width of the lighting sliders (label -> slider -> value, one rhythm).
const SLIDER_W: f32 = 240.0;

pub fn render_led_panel(ui: &mut egui::Ui, app: &mut PanteonApp) {
    let opt = LED_EFFECT_OPTIONS[app.selected_effect_idx];

    ui.horizontal(|ui| {
        ui.add_sized([CONTROL_LABEL_W, 20.0], egui::Label::new("Эффект"));
        egui::ComboBox::from_id_salt("led_effect_combo")
            .selected_text(opt.label)
            .show_ui(ui, |ui| {
                for (i, item) in LED_EFFECT_OPTIONS.iter().enumerate() {
                    ui.selectable_value(&mut app.selected_effect_idx, i, item.label);
                }
            });
    });

    ui.add_space(8.0);

    // Speed: label (90px) -> slider without .text() -> value.
    let speed_tip = if opt.spec.supports_speed {
        "Скорость анимации: 0 — минимум, 5 — максимум"
    } else {
        "Эффект без регулировки скорости"
    };
    ui.horizontal(|ui| {
        ui.add_sized([CONTROL_LABEL_W, 20.0], egui::Label::new("Скорость"));
        ui.spacing_mut().slider_width = SLIDER_W;
        let resp = styled_slider_enabled(
            ui,
            opt.spec.supports_speed,
            egui::Slider::new(&mut app.led_speed, 0..=5),
        );
        if opt.spec.supports_speed {
            theme::tooltip(resp, speed_tip);
        } else {
            theme::disabled_tooltip(resp, speed_tip);
        }
    });
    if !opt.spec.supports_speed {
        ui.horizontal(|ui| {
            ui.add_space(CONTROL_LABEL_W + 8.0);
            ui.label(
                egui::RichText::new("Эффект без регулировки скорости")
                    .size(12.0)
                    .weak(),
            );
        });
    }

    ui.add_space(8.0);

    // Brightness: 8 positions (wire levels 1..=8); the left position is the real
    // «off» (0%), so the label is the semantic percent from `codec`, never a naive
    // linear level scale.
    // Register is global (active-low Bank 7 @ 0x0038), so the slider is unconditional.
    ui.horizontal(|ui| {
        ui.add_sized([CONTROL_LABEL_W, 20.0], egui::Label::new("Яркость"));
        ui.spacing_mut().slider_width = SLIDER_W;
        let resp = styled_slider(
            ui,
            egui::Slider::new(&mut app.led_brightness, 1..=8).custom_formatter(|val, _| {
                format!("{}%", crate::codec::brightness_percent(val as u8))
            }),
        );
        let bank7 = crate::codec::brightness_to_bank7(app.led_brightness)
            .map(|v| format!("0x{:04x}", v))
            .unwrap_or_else(|_| "—".to_string());
        theme::tooltip(
            resp,
            format!(
                "0% — подсветка выключена, 100% — максимум\nWire-уровень {} из 8; Bank 7 @ 0x0038 = {}",
                app.led_brightness, bank7
            ),
        );
    });

    ui.add_space(8.0);

    // Palette section with dynamic enable/disable
    if opt.spec.supports_palette {
        ui.horizontal(|ui| {
            let resp = ui.checkbox(&mut app.all_colors, "Все цвета");
            let mask = encode_palette_mask(app.all_colors, &app.selected_colors);
            theme::tooltip(
                resp,
                format!("Маска палитры: 0x{:02x}\n• 0xff — все 7 цветов RGB", mask),
            );
        });

        if !app.all_colors {
            ui.horizontal_wrapped(|ui| {
                for (i, name) in PALETTE_COLOR_NAMES.iter().enumerate() {
                    ui.checkbox(&mut app.selected_colors[i], *name);
                }
            });
        }
    } else {
        ui.label(egui::RichText::new("Палитра фиксирована для этого эффекта").weak());
    }
}

// ---------------------------------------------------------------------------
// Tab 4: Профили
// ---------------------------------------------------------------------------

pub fn render_profiles_tab(ui: &mut egui::Ui, app: &mut PanteonApp) {
    card(ui, |ui| {
        card_title(
            ui,
            "Профили",
            Some("Сохранённые профили. Загрузка меняет панель — применяйте кнопкой в шапке."),
        );
        crate::gui::render_preset_panel(ui, app);
    });
    ui.add_space(theme::GAP);

    card(ui, |ui| {
        card_title(ui, "Снимки состояния", None);
        crate::gui::render_snapshot_panel(ui, app);

        if let Some(summary) = &app.dry_run_summary {
            if summary.title.contains("снимка") || summary.title.contains("Восстановление") {
                ui.add_space(8.0);
                render_dry_run_summary(ui, summary);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Confirm Dialog: Записать в память мыши (R10: без чекбокса)
// ---------------------------------------------------------------------------

pub fn render_confirm_dialog(ctx: &egui::Context, app: &mut PanteonApp) {
    if !app.confirm_modal_open {
        return;
    }

    egui::Window::new("Записать в память мыши")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.colored_label(
                theme::DANGER,
                "Внимание: Вы собираетесь выполнить запись во Flash-память мыши.",
            );
            ui.label("Уровень гейта: Уровень 2 (Запись конфигурации SPI Flash)");

            if !app.pending_dirty_categories.is_empty() {
                let cat_names: Vec<&str> = app
                    .pending_dirty_categories
                    .iter()
                    .map(|c| c.label())
                    .collect();
                ui.label(format!("Изменяемые категории: {}", cat_names.join(", ")));
            }

            let mut total_frames_count = 0;
            let mut sample_frame_str = String::new();
            let mut all_frame_lines = Vec::new();

            if !app.pending_blocks.is_empty() {
                ui.separator();
                let addrs: Vec<String> = app
                    .pending_blocks
                    .iter()
                    .map(|b| format!("0x{:04x}", b.addr))
                    .collect();
                ui.label(format!("Адреса регистров: {}", addrs.join(", ")));

                if let Some(b38) = app.pending_blocks.iter().find(|b| b.addr == 0x0038) {
                    ui.label(format!("Яркость LED (Bank 7 @ 0x0038): 0x{:04x}", b38.banks[7]));
                }

                let mut banks_set = std::collections::BTreeSet::new();
                for b in &app.pending_blocks {
                    for (i, _) in b.banks.iter().enumerate() {
                        banks_set.insert(i);
                    }
                }
                let banks_str = banks_set
                    .into_iter()
                    .map(|b| b.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                ui.label(format!("Затрагиваемые банки Flash: {}", banks_str));

                let raw_frames = crate::service::plan_block_session_frames(&app.pending_blocks, None, false);
                total_frames_count = raw_frames.len();
                sample_frame_str = raw_frames
                    .first()
                    .map(|f| hex_format(f))
                    .unwrap_or_default();
                all_frame_lines = raw_frames
                    .iter()
                    .enumerate()
                    .map(|(i, f)| format!("[{:04}] {}", i, hex_format(f)))
                    .collect();
            } else if let Some(plan) = &app.pending_plan {
                ui.separator();
                ui.label(format!(
                    "Затрагиваемые банки: Банк {} (регистр 0x{:04x}, данные 0x{:04x})",
                    plan.bank, plan.addr, plan.data
                ));
                all_frame_lines = plan.log_frames();
                total_frames_count = all_frame_lines.len();
                sample_frame_str = hex_format(&plan.data_frame);
            } else if !app.pending_plans.is_empty() {
                ui.separator();
                ui.label(format!(
                    "Затрагиваемые банки: 0, 1, 2, 3, 4, 5, 6, 7 (полная конфигурация DPI, {} планов)",
                    app.pending_plans.len()
                ));
                for p in &app.pending_plans {
                    for line in p.log_frames() {
                        all_frame_lines.push(line);
                    }
                }
                total_frames_count = all_frame_lines.len();
                sample_frame_str = app
                    .pending_plans
                    .first()
                    .map(|p| hex_format(&p.data_frame))
                    .unwrap_or_default();
            } else if let Some(delay_ms) = app.pending_fire_delay {
                ui.separator();
                ui.label(format!(
                    "Затрагиваемые банки: Банки 2, 3 (регистры 0x0018, отдача {} мс)",
                    delay_ms
                ));
                if let Ok((p_low, p_high)) = crate::service::plan_fire_delay(delay_ms) {
                    all_frame_lines.extend(p_low.log_frames());
                    all_frame_lines.extend(p_high.log_frames());
                    total_frames_count = all_frame_lines.len();
                    sample_frame_str = hex_format(&p_low.data_frame);
                }
            } else if let Some(snap) = &app.pending_snapshot {
                ui.separator();
                ui.label(format!(
                    "Затрагиваемые банки: 0, 1, 2, 3, 4, 5, 6, 7 (образ '{}', PID 0x{:04x})",
                    snap.label, snap.source_pid
                ));
                total_frames_count = snap.frames.len();
                sample_frame_str = snap
                    .frames
                    .first()
                    .map(|f| hex_format(f))
                    .unwrap_or_default();
                all_frame_lines = snap
                    .frames
                    .iter()
                    .enumerate()
                    .map(|(i, f)| format!("[{:04}] {}", i, hex_format(f)))
                    .collect();
            }

            ui.add_space(4.0);
            ui.label(format!("Число кадров USB: {}", total_frames_count));
            if !sample_frame_str.is_empty() {
                ui.label(format!("Пример кадра [0000]: {}", sample_frame_str));
            }

            ui.add_space(4.0);

            // Collapsible header for USB frames: collapsed by default, mono font ONLY here!
            egui::CollapsingHeader::new(format!("Кадры USB ({})", total_frames_count))
                .default_open(false)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(140.0)
                        .show(ui, |ui| {
                            for line in &all_frame_lines {
                                ui.monospace(line);
                            }
                        });
                });

            ui.separator();
            ui.add_space(4.0);

            // Confirm buttons: [Записать] and [Отмена] (No checkbox!)
            ui.horizontal(|ui| {
                if ui.button("Отмена").clicked() {
                    app.cancel_confirm();
                }

                let write_btn = egui::Button::new(
                    egui::RichText::new("Записать")
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(theme::DANGER);

                if ui.add(write_btn).clicked() {
                    app.execute_confirmed_write();
                }
            });
        });
}
