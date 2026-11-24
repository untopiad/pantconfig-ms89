use eframe::egui;

use crate::codec::DescriptorPage;
use crate::device::{access, AccessReport, BlockWrite, DiscoveredDevice, WriteGateLevel, WriteTransactionPlan};
use crate::gui::access_panel;
use crate::gui::panels::*;
use crate::gui::theme;
use crate::model::{registers, DeviceIdentity, DeviceState, Profile, ShadowLedger, StateSnapshot};
use crate::service::{
    discover, load_ledger, open_device, read_descriptor, read_handshake, resolve_state_path,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GuiTab {
    #[default]
    State,     // Состояние
    Dpi,       // DPI
    Lighting,  // Подсветка
    Profiles,  // Профили
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DirtyCategory {
    Dpi,
    Colors,
    Polling,
    FireDelay,
    Led,
    ActiveSlot,
    Profiles,
}

impl DirtyCategory {
    pub fn label(&self) -> &'static str {
        match self {
            DirtyCategory::Dpi => "DPI",
            DirtyCategory::Colors => "Цвета",
            DirtyCategory::Polling => "Частота опроса",
            DirtyCategory::FireDelay => "Отдача",
            DirtyCategory::Led => "Подсветка",
            DirtyCategory::ActiveSlot => "Активный слот",
            DirtyCategory::Profiles => "Профиль",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppBaseline {
    pub selected_polling: u16,
    pub selected_dpi_slot: u8,
    pub active_slot_override: Option<u8>,
    pub dpi_slots: [u32; 6],
    pub slot_colors: [[u8; 3]; 6],
    pub fire_delay: u16,
    pub selected_effect_idx: usize,
    pub led_speed: u8,
    pub led_brightness: u8,
    pub all_colors: bool,
    pub selected_colors: [bool; 7],
    pub snapshot_label: Option<String>,
}

impl Default for AppBaseline {
    fn default() -> Self {
        Self {
            selected_polling: 1000,
            selected_dpi_slot: 1,
            active_slot_override: None,
            dpi_slots: [200, 400, 800, 1600, 3200, 6400],
            slot_colors: [
                [255, 255, 0],
                [0, 0, 255],
                [255, 0, 255],
                [0, 255, 0],
                [255, 0, 0],
                [0, 255, 255],
            ],
            fire_delay: 50,
            selected_effect_idx: 0,
            led_speed: 3,
            led_brightness: 4,
            all_colors: true,
            selected_colors: [false; 7],
            snapshot_label: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Success, // Успех
    Plan,    // План
    Error,   // Ошибка
    Warning, // Предупреждение
}

impl StatusKind {
    pub fn label(&self) -> &'static str {
        match self {
            StatusKind::Success => "Успех",
            StatusKind::Plan => "План",
            StatusKind::Error => "Ошибка",
            StatusKind::Warning => "Предупреждение",
        }
    }

    pub fn color(&self) -> egui::Color32 {
        match self {
            StatusKind::Success => crate::gui::theme::SUCCESS,
            StatusKind::Plan => crate::gui::theme::INFO,
            StatusKind::Error => crate::gui::theme::DANGER,
            StatusKind::Warning => crate::gui::theme::WARN,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppStatus {
    pub kind: StatusKind,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct DryRunSummary {
    pub title: String,
    pub description: String,
    pub frame_count: usize,
    pub sample_frame: String,
    pub all_frames: Vec<String>,
}

fn hex_format(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

pub struct PanteonApp {
    pub devices: Vec<DiscoveredDevice>,
    pub selected_device_idx: Option<usize>,
    pub device_state: Option<DeviceState>,
    pub descriptor_page: Option<DescriptorPage>,
    pub ledger: ShadowLedger,
    pub status_message: String,
    pub status: AppStatus,
    pub active_tab: GuiTab,
    pub dry_run_summary: Option<DryRunSummary>,

    // Dirty-state & Baseline tracking (R10)
    pub baseline: AppBaseline,
    pub diff_dialog_open: bool,

    // Controls
    pub selected_polling: u16,
    pub selected_dpi_slot: u8,
    pub active_slot_override: Option<u8>,
    pub dpi_slots: [u32; 6],
    pub slot_colors: [[u8; 3]; 6],
    /// Слот, у которого сейчас раскрыта палитра пресетов (только один за раз)
    pub dpi_palette_slot: Option<usize>,
    pub fire_delay: u16,
    pub selected_effect_idx: usize,
    pub led_speed: u8,
    pub led_brightness: u8,
    pub all_colors: bool,
    pub selected_colors: [bool; 7],

    // Snapshot
    pub snapshot_path: String,
    /// Inline name input for saving the current configuration as a named profile preset
    pub preset_name_input: String,
    /// R16: preset name pending an overwrite confirmation (UI-only, never in baseline)
    pub preset_overwrite_confirm: Option<String>,
    /// R16: preset name pending a delete confirmation (UI-only, never in baseline)
    pub preset_delete_confirm: Option<String>,
    /// R17: currently selected preset in the profile selector (UI-only)
    pub selected_preset: Option<String>,
    /// R17: «Новый профиль» modal is open (UI-only)
    pub preset_new_modal_open: bool,
    /// R17: preset the user wants to load while the editor has unsaved edits (UI-only)
    pub preset_switch_confirm: Option<String>,
    /// R18: the selected preset as it is stored on disk, captured at load/save time.
    /// UI-only (never in `AppBaseline`); drives the «Сохранить» ACCENT indicator.
    pub selected_preset_snapshot: Option<Profile>,
    pub snapshot_preview: Option<StateSnapshot>,
    pub snapshot_error: Option<String>,

    // Safety Gate & Confirm Modal
    pub confirm_modal_open: bool,
    pub pending_plan: Option<WriteTransactionPlan>,
    pub pending_plans: Vec<WriteTransactionPlan>,
    pub pending_blocks: Vec<BlockWrite>,
    pub pending_dirty_categories: Vec<DirtyCategory>,
    pub pending_fire_delay: Option<u16>,
    pub pending_snapshot: Option<StateSnapshot>,

    // R23: read-only access diagnostics. UI-only: deliberately NOT part of
    // `AppBaseline` (AGENTS §1.6) and never refreshed per frame.
    pub access_report: Option<AccessReport>,
    pub access_modal_open: bool,
}

impl Default for PanteonApp {
    fn default() -> Self {
        let initial_text = "Готово. Выберите устройство для проверки состояния.".to_string();
        let mut app = Self {
            devices: Vec::new(),
            selected_device_idx: None,
            device_state: None,
            descriptor_page: None,
            ledger: ShadowLedger::default(),
            status_message: initial_text.clone(),
            status: AppStatus {
                kind: StatusKind::Success,
                text: initial_text,
            },
            active_tab: GuiTab::State,
            dry_run_summary: None,
            baseline: AppBaseline::default(),
            diff_dialog_open: false,
            selected_polling: 1000,
            selected_dpi_slot: 1,
            active_slot_override: None,
            dpi_slots: [200, 400, 800, 1600, 3200, 6400],
            dpi_palette_slot: None,
            slot_colors: [
                [255, 255, 0],
                [0, 0, 255],
                [255, 0, 255],
                [0, 255, 0],
                [255, 0, 0],
                [0, 255, 255],
            ],
            fire_delay: 50,
            selected_effect_idx: 0,
            led_speed: 3,
            led_brightness: 4,
            all_colors: true,
            selected_colors: [false; 7],
            snapshot_path: String::new(),
            preset_name_input: String::new(),
            preset_overwrite_confirm: None,
            preset_delete_confirm: None,
            selected_preset: None,
            preset_new_modal_open: false,
            preset_switch_confirm: None,
            selected_preset_snapshot: None,
            snapshot_preview: None,
            snapshot_error: None,
            confirm_modal_open: false,
            pending_plan: None,
            pending_plans: Vec::new(),
            pending_blocks: Vec::new(),
            pending_dirty_categories: Vec::new(),
            pending_fire_delay: None,
            pending_snapshot: None,
            access_report: None,
            access_modal_open: false,
        };
        app.sync_baseline();
        app
    }
}

impl PanteonApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::default();
        app.refresh_devices();
        app.reload_ledger();
        if let Ok(tab) = std::env::var("PANTEON_TAB") {
            match tab.to_lowercase().as_str() {
                "state" => app.active_tab = GuiTab::State,
                "dpi" => app.active_tab = GuiTab::Dpi,
                "dpi_dirty" => {
                    app.active_tab = GuiTab::Dpi;
                    app.selected_polling = if app.selected_polling == 250 { 500 } else { 250 };
                }
                "lighting" | "light" | "led" => app.active_tab = GuiTab::Lighting,
                "profiles" | "profile" => app.active_tab = GuiTab::Profiles,
                "confirm" => {
                    app.active_tab = GuiTab::Dpi;
                    app.selected_polling = if app.selected_polling == 250 { 500 } else { 250 };
                    app.request_cumulative_apply();
                }
                _ => {}
            }
        }
        // Screenshot/test hook (like PANTEON_TAB/PANTEON_HOVER): open the
        // access modal deterministically without synthesising a click.
        if std::env::var("PANTEON_ACCESS_MODAL").ok().as_deref() == Some("1") {
            app.access_modal_open = true;
        }
        app
    }

    pub fn set_status(&mut self, kind: StatusKind, text: impl Into<String>) {
        let msg = text.into();
        self.status_message = msg.clone();
        self.status = AppStatus { kind, text: msg };
    }

    pub fn current_status(&self) -> (StatusKind, String) {
        if self.status_message == self.status.text {
            (self.status.kind, self.status.text.clone())
        } else {
            let kind = if self.status_message.starts_with("[Dry-run Plan]") {
                StatusKind::Plan
            } else if self.status_message.contains("error")
                || self.status_message.contains("Error")
                || self.status_message.contains("Failed")
                || self.status_message.contains("blocked")
            {
                StatusKind::Error
            } else if self.status_message.contains("Warning")
                || self.status_message.contains("WARNING")
                || self.status_message.contains("unknown")
            {
                StatusKind::Warning
            } else {
                StatusKind::Success
            };
            (kind, self.status_message.clone())
        }
    }

    pub fn rgb_colors(&self) -> [(u8, u8, u8); 6] {
        let mut res = [(0, 0, 0); 6];
        for (i, c) in self.slot_colors.iter().enumerate() {
            res[i] = (c[0], c[1], c[2]);
        }
        res
    }

    /// Discards all unsaved edits: restores editable fields from the baseline snapshot.
    /// Also resets the dry-run summary and pending state (nothing was written).
    pub fn discard_changes(&mut self) {
        let b = self.baseline.clone();
        self.selected_polling = b.selected_polling;
        self.selected_dpi_slot = b.selected_dpi_slot;
        self.active_slot_override = b.active_slot_override;
        self.dpi_slots = b.dpi_slots;
        self.slot_colors = b.slot_colors;
        self.fire_delay = b.fire_delay;
        self.selected_effect_idx = b.selected_effect_idx;
        self.led_speed = b.led_speed;
        self.led_brightness = b.led_brightness;
        self.all_colors = b.all_colors;
        self.selected_colors = b.selected_colors;
        self.dry_run_summary = None;
        self.set_status(StatusKind::Plan, "Изменения сброшены к последнему применённому состоянию.");
    }

    pub fn sync_baseline(&mut self) {
        if let Some(entry) = self
            .ledger
            .entries
            .iter()
            .rev()
            .find(|e| e.bank == 7 && e.addr == 0x0038)
        {
            let nibble = ((entry.data & 0xff) >> 4) as u8;
            if nibble <= 8 {
                // Nibble 8 (level 0) is outside the OEM domain and lights as 100%.
                self.led_brightness = crate::codec::normalize_brightness_level(8 - nibble);
            }
        }

        self.baseline = AppBaseline {
            selected_polling: self.selected_polling,
            selected_dpi_slot: self.selected_dpi_slot,
            active_slot_override: self.active_slot_override,
            dpi_slots: self.dpi_slots,
            slot_colors: self.slot_colors,
            fire_delay: self.fire_delay,
            selected_effect_idx: self.selected_effect_idx,
            led_speed: self.led_speed,
            led_brightness: self.led_brightness,
            all_colors: self.all_colors,
            selected_colors: self.selected_colors,
            snapshot_label: self.snapshot_preview.as_ref().map(|s| s.label.clone()),
        };
    }

    pub fn dirty_categories(&self) -> Vec<DirtyCategory> {
        let mut cats = Vec::new();
        if self.dpi_slots != self.baseline.dpi_slots {
            cats.push(DirtyCategory::Dpi);
        }
        if self.slot_colors != self.baseline.slot_colors {
            cats.push(DirtyCategory::Colors);
        }
        if self.selected_polling != self.baseline.selected_polling {
            cats.push(DirtyCategory::Polling);
        }
        if self.fire_delay != self.baseline.fire_delay {
            cats.push(DirtyCategory::FireDelay);
        }
        if self.selected_effect_idx != self.baseline.selected_effect_idx
            || self.led_speed != self.baseline.led_speed
            || self.led_brightness != self.baseline.led_brightness
            || self.all_colors != self.baseline.all_colors
            || self.selected_colors != self.baseline.selected_colors
        {
            cats.push(DirtyCategory::Led);
        }
        if self.selected_dpi_slot != self.baseline.selected_dpi_slot
            || self.active_slot_override != self.baseline.active_slot_override
        {
            cats.push(DirtyCategory::ActiveSlot);
        }
        if self.snapshot_preview.as_ref().map(|s| &s.label) != self.baseline.snapshot_label.as_ref() {
            cats.push(DirtyCategory::Profiles);
        }
        cats
    }

    pub fn is_dirty(&self) -> bool {
        !self.dirty_categories().is_empty()
    }

    pub fn diff_entries(&self) -> Vec<(String, String, String)> {
        let mut diffs = Vec::new();
        if self.selected_polling != self.baseline.selected_polling {
            diffs.push((
                "Частота опроса".to_string(),
                format!("{} Гц", self.baseline.selected_polling),
                format!("{} Гц", self.selected_polling),
            ));
        }
        if self.selected_dpi_slot != self.baseline.selected_dpi_slot {
            diffs.push((
                "Активный слот DPI".to_string(),
                format!("Слот {}", self.baseline.selected_dpi_slot),
                format!("Слот {}", self.selected_dpi_slot),
            ));
        }
        if self.active_slot_override != self.baseline.active_slot_override {
            let old_str = self
                .baseline
                .active_slot_override
                .map(|s| format!("Слот {}", s))
                .unwrap_or_else(|| "Авто".to_string());
            let new_str = self
                .active_slot_override
                .map(|s| format!("Слот {}", s))
                .unwrap_or_else(|| "Авто".to_string());
            diffs.push(("Переопределение слота".to_string(), old_str, new_str));
        }
        if self.fire_delay != self.baseline.fire_delay {
            diffs.push((
                "Задержка кнопки огня".to_string(),
                format!("{} мс", self.baseline.fire_delay),
                format!("{} мс", self.fire_delay),
            ));
        }
        if self.led_brightness != self.baseline.led_brightness {
            diffs.push((
                "Яркость LED".to_string(),
                format!(
                    "{} ({:.1}%)",
                    self.baseline.led_brightness,
                    crate::codec::brightness_percent(self.baseline.led_brightness)
                ),
                format!(
                    "{} ({:.1}%)",
                    self.led_brightness,
                    crate::codec::brightness_percent(self.led_brightness)
                ),
            ));
        }
        if self.selected_effect_idx != self.baseline.selected_effect_idx {
            diffs.push((
                "Эффект подсветки".to_string(),
                crate::gui::LED_EFFECT_OPTIONS[self.baseline.selected_effect_idx]
                    .label
                    .to_string(),
                crate::gui::LED_EFFECT_OPTIONS[self.selected_effect_idx]
                    .label
                    .to_string(),
            ));
        }
        if self.led_speed != self.baseline.led_speed {
            diffs.push((
                "Скорость подсветки".to_string(),
                format!("{}", self.baseline.led_speed),
                format!("{}", self.led_speed),
            ));
        }
        if self.all_colors != self.baseline.all_colors {
            diffs.push((
                "Все цвета палитры".to_string(),
                if self.baseline.all_colors { "Да" } else { "Нет" }.to_string(),
                if self.all_colors { "Да" } else { "Нет" }.to_string(),
            ));
        }
        for i in 0..6 {
            if self.dpi_slots[i] != self.baseline.dpi_slots[i] {
                diffs.push((
                    format!("DPI Слот {}", i + 1),
                    format!("{} DPI", self.baseline.dpi_slots[i]),
                    format!("{} DPI", self.dpi_slots[i]),
                ));
            }
            if self.slot_colors[i] != self.baseline.slot_colors[i] {
                diffs.push((
                    format!("Цвет Слот {}", i + 1),
                    format!(
                        "#{:02x}{:02x}{:02x}",
                        self.baseline.slot_colors[i][0],
                        self.baseline.slot_colors[i][1],
                        self.baseline.slot_colors[i][2]
                    ),
                    format!(
                        "#{:02x}{:02x}{:02x}",
                        self.slot_colors[i][0],
                        self.slot_colors[i][1],
                        self.slot_colors[i][2]
                    ),
                ));
            }
        }
        if self.snapshot_preview.as_ref().map(|s| &s.label) != self.baseline.snapshot_label.as_ref() {
            diffs.push((
                "Снимок профиля".to_string(),
                self.baseline
                    .snapshot_label
                    .clone()
                    .unwrap_or_else(|| "—".to_string()),
                self.snapshot_preview
                    .as_ref()
                    .map(|s| s.label.clone())
                    .unwrap_or_else(|| "—".to_string()),
            ));
        }
        diffs
    }

    pub fn build_editor_full_push_snapshot(&self) -> Result<StateSnapshot, crate::PanteonError> {
        let mut push_ledger = self.ledger.clone();
        let b38 = crate::service::resolve_block38(&self.ledger, Some(self.led_brightness))?;
        push_ledger.append(crate::model::ShadowEntry::new(
            7,
            0x0038,
            b38.banks[7],
            crate::model::current_timestamp_secs(),
        ));
        let active_slot = self.active_slot_override.unwrap_or(self.selected_dpi_slot);
        let slot_val = 0x0718 | (active_slot as u16 - 1);
        push_ledger.append(crate::model::ShadowEntry::new(
            0,
            0x0018,
            slot_val,
            crate::model::current_timestamp_secs(),
        ));
        crate::service::build_full_push_snapshot(&push_ledger)
    }

    pub fn plan_cumulative_block_groups(
        &self,
    ) -> Result<(Option<BlockWrite>, Vec<BlockWrite>), crate::PanteonError> {
        let dirty = self.dirty_categories();
        let mut block_18 = None;
        let mut blocks_dpi_color = Vec::new();

        let has_led_changes = dirty.contains(&DirtyCategory::Led)
            || self.led_brightness != self.baseline.led_brightness;

        // Group 1: 0x0018
        let needs_18 = dirty.contains(&DirtyCategory::Polling)
            || dirty.contains(&DirtyCategory::ActiveSlot)
            || dirty.contains(&DirtyCategory::FireDelay)
            || has_led_changes;

        if needs_18 {
            let mut overrides_18 = Vec::new();
            if let Ok(p) = crate::service::plan_polling_rate(self.selected_polling) {
                overrides_18.push((p.bank, p.data));
            }
            let slot = self.active_slot_override.unwrap_or(self.selected_dpi_slot);
            if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(slot) {
                overrides_18.push((registers::ACTIVE_DPI_SLOT.0, raw_slot));
            }
            overrides_18.push((registers::FIRE_DELAY_LOW.0, 0x0700 | (self.fire_delay & 0xff)));
            overrides_18.push((registers::FIRE_DELAY_HIGH.0, 0x0700 | ((self.fire_delay >> 8) & 0xff)));

            let opt = &crate::gui::LED_EFFECT_OPTIONS[self.selected_effect_idx];
            let wire_byte = opt.spec.wire_byte(self.led_speed);
            overrides_18.push((registers::LED_EFFECT_SPEED.0, 0x0700 | wire_byte as u16));
            let mask = crate::codec::led::encode_palette_mask(self.all_colors, &self.selected_colors);
            overrides_18.push((registers::LED_COLOR_PALETTE.0, 0x0700 | mask as u16));

            let b18 = crate::service::resolve_block18(&self.ledger, &overrides_18)?;
            block_18 = Some(b18);
        }

        // Group 2: 0x30/0x38 (DPI & Colors & Brightness)
        let needs_dpi_color = dirty.contains(&DirtyCategory::Dpi) || dirty.contains(&DirtyCategory::Colors);
        if needs_dpi_color {
            let dpi_plans = crate::service::plan_dpi_slots(&self.dpi_slots)?;
            let colors = self.rgb_colors();
            let color_plans = crate::service::plan_color_session(
                &colors,
                &crate::codec::DEFAULT_SECOND_COLORS,
                &self.ledger,
                Some(self.led_brightness),
            )?;
            let mut all_plans = dpi_plans.to_vec();
            all_plans.extend(color_plans);
            blocks_dpi_color = crate::service::plans_to_block_writes(&all_plans);
        } else if has_led_changes {
            let b38 = crate::service::resolve_block38(&self.ledger, Some(self.led_brightness))?;
            blocks_dpi_color.push(b38);
        }

        Ok((block_18, blocks_dpi_color))
    }

    pub fn request_cumulative_apply(&mut self) {
        self.clear_pending();
        let dirty = self.dirty_categories();
        if dirty.is_empty() {
            return;
        }

        if dirty.contains(&DirtyCategory::Profiles) && self.snapshot_preview.is_some() {
            match self.build_editor_full_push_snapshot() {
                Ok(mut snap) => {
                    if let Some(prev) = &self.snapshot_preview {
                        snap.label = format!("profile-{}", prev.label);
                    }
                    self.pending_snapshot = Some(snap);
                }
                Err(e) => {
                    self.set_status(StatusKind::Error, format!("Ошибка сборки снимка: {}", e));
                    return;
                }
            }
        } else {
            match self.plan_cumulative_block_groups() {
                Ok((b18_opt, dpi_color_blocks)) => {
                    let mut blocks = Vec::new();
                    if let Some(b18) = b18_opt {
                        blocks.push(b18);
                    }
                    blocks.extend(dpi_color_blocks);
                    self.pending_blocks = blocks;
                }
                Err(e) => {
                    self.set_status(StatusKind::Error, format!("Ошибка формирования плана: {}", e));
                    return;
                }
            }
        }

        self.pending_dirty_categories = dirty;
        self.confirm_modal_open = true;
    }

    pub fn refresh_devices(&mut self) {
        match discover() {
            Ok(devs) => {
                self.devices = devs;
                let unique_count = {
                    let mut unique_keys = std::collections::HashSet::new();
                    for d in &self.devices {
                        unique_keys.insert((d.vendor_id, d.product_id, d.usb_bus, d.usb_dev));
                    }
                    unique_keys.len()
                };
                if !self.devices.is_empty() && self.selected_device_idx.is_none() {
                    self.selected_device_idx = Some(0);
                    self.query_device_state();
                } else if self.devices.is_empty() {
                    self.selected_device_idx = None;
                    self.device_state = None;
                    self.descriptor_page = None;
                }
                self.set_status(
                    StatusKind::Success,
                    format!("Найдено устройств: {}", unique_count),
                );
            }
            Err(e) => {
                self.set_status(
                    StatusKind::Error,
                    format!("Ошибка обнаружения устройств: {}", e),
                );
            }
        }
        self.refresh_access_report();
    }

    /// R23: recompute access diagnostics. Called on start, after device refresh
    /// and from the modal's «Перепроверить» button — never per frame.
    pub fn refresh_access_report(&mut self) {
        self.access_report = Some(access::diagnose(&self.devices));
    }

    pub fn reload_ledger(&mut self) {
        let state_path = resolve_state_path(None);
        self.ledger = load_ledger(&state_path).unwrap_or_default();
        self.project_editor_from_ledger();
        self.sync_baseline();
    }

    /// Latest written value for a register in the shadow ledger (what the device holds).
    fn ledger_latest(&self, bank: u8, addr: u16) -> Option<u16> {
        self.ledger
            .entries
            .iter()
            .rev()
            .find(|e| e.bank == bank && e.addr == addr)
            .map(|e| e.data)
    }

    /// Projects the shadow ledger (the last known device state) onto the editor fields.
    ///
    /// `sync_baseline()` then snapshots this projection, so «dirty» means exactly
    /// «the editor differs from the device». Before this, only the DPI slots, the
    /// active slot and the brightness were derived from the ledger; polling rate,
    /// LED effect/speed, palette mask and fire delay kept their *defaults*, so
    /// loading a profile that matched the ledger still lit up «Применить»
    /// (e.g. baseline «1. DPI дыхание, 3» vs device «6. Комета, 2»).
    ///
    /// Deliberately NOT projected: DPI slot colours. The ledger's colour blocks
    /// cannot be decoded back reliably (feeding the captured 0x20/0x28/0x30 values
    /// to `codec::decode_slot_colors` yields a different palette than was written),
    /// and per ADR-002 the device cannot be read back. They keep the factory
    /// default: a profile with custom colours still reports a pending «Цвета»
    /// change, which is the honest answer.
    fn project_editor_from_ledger(&mut self) {
        // DPI levels: banks 3, 5, 6, 7 @ 0x0010.
        if let (Some(b3), Some(b5), Some(b6), Some(b7)) = (
            self.ledger_latest(3, 0x0010),
            self.ledger_latest(5, 0x0010),
            self.ledger_latest(6, 0x0010),
            self.ledger_latest(7, 0x0010),
        ) {
            if let Ok(slots) =
                crate::codec::decode_dpi_slots((b3 & 0xff) as u8, (b5 & 0xff) as u8, (b6 & 0xff) as u8, (b7 & 0xff) as u8)
            {
                self.dpi_slots = slots;
            }
        }

        // Active DPI slot: bank 0 @ 0x0018 (raw 0x18..0x1d -> slot 1..6).
        if let Some(active) = self.ledger_latest(registers::ACTIVE_DPI_SLOT.0, registers::ACTIVE_DPI_SLOT.1) {
            let raw = (active & 0xff) as u8;
            if (0x18..=0x1d).contains(&raw) {
                self.selected_dpi_slot = raw - 0x18 + 1;
            }
        }

        // Polling rate: bank 6 @ 0x0018 (confirmed register codes).
        if let Some(code) = self.ledger_latest(registers::POLLING_RATE.0, registers::POLLING_RATE.1) {
            if let Some(rate) = crate::model::PollingRate::from_register_code(code) {
                self.selected_polling = rate.as_hz();
            }
        }

        // LED effect + speed: bank 4 @ 0x0018, encoded as one wire byte. Decoding
        // yields the semantic mode_index; the dropdown position is found explicitly.
        if let Some(data) = self.ledger_latest(registers::LED_EFFECT_SPEED.0, registers::LED_EFFECT_SPEED.1) {
            if let Some((mode_index, speed)) = crate::codec::led::decode_effect_byte((data & 0xff) as u8) {
                if let Some(idx) = crate::gui::LED_EFFECT_OPTIONS
                    .iter()
                    .position(|o| o.spec.mode_index == mode_index)
                {
                    self.selected_effect_idx = idx;
                    self.led_speed = speed;
                }
            }
        }

        // Palette mask: bank 5 @ 0x0018 (0xff = all colours).
        if let Some(data) = self.ledger_latest(registers::LED_COLOR_PALETTE.0, registers::LED_COLOR_PALETTE.1) {
            let (all_colors, selected_colors) = crate::codec::led::decode_palette_mask((data & 0xff) as u8);
            self.all_colors = all_colors;
            self.selected_colors = selected_colors;
        }

        // Brightness: bank 7 @ 0x0038 (active-low nibble).
        if let Some(data) = self.ledger_latest(registers::LED_BRIGHTNESS.0, registers::LED_BRIGHTNESS.1) {
            if let Ok(level) = crate::codec::bank7_to_brightness(data) {
                self.led_brightness = crate::codec::normalize_brightness_level(level);
            }
        }

        // Fire delay: low byte in bank 2, high byte in bank 3 (both @ 0x0018).
        // Applied only inside the slider range; a missing/zero entry means «unknown»
        // and keeps the default instead of inventing a 0 ms delay.
        if let Some(low) = self.ledger_latest(registers::FIRE_DELAY_LOW.0, registers::FIRE_DELAY_LOW.1) {
            let high = self
                .ledger_latest(registers::FIRE_DELAY_HIGH.0, registers::FIRE_DELAY_HIGH.1)
                .unwrap_or(0x0700);
            let delay = fire_delay_from_registers(low, high);
            if (8..=300).contains(&delay) {
                self.fire_delay = delay;
            }
        }
    }

    pub fn current_device(&self) -> Option<&DiscoveredDevice> {
        self.selected_device_idx.and_then(|idx| self.devices.get(idx))
    }

    pub fn current_identity(&self) -> Option<&DeviceIdentity> {
        self.current_device().map(|d| &d.identity)
    }

    pub fn query_device_state(&mut self) {
        if let Some(idx) = self.selected_device_idx {
            if idx < self.devices.len() {
                let path = self.devices[idx].path.clone();
                match open_device(Some(path), WriteGateLevel::Level0ReadOnly) {
                    Ok((_, mut dev)) => {
                        self.device_state = read_handshake(&mut dev).ok();
                        if let Some(state) = &self.device_state {
                            if let Some(hs) = state.handshake {
                                if !self.devices[idx].identity.protocol_verified
                                    && panteon_ms89::device::check_persisted_verification_with_signature(
                                        self.devices[idx].product_id,
                                        hs.signature,
                                    )
                                {
                                    self.devices[idx].identity.protocol_verified = true;
                                }
                            }
                        }
                        self.descriptor_page = read_descriptor(&mut dev).ok();
                        self.set_status(
                            StatusKind::Success,
                            "Дескриптор и состояние устройства успешно получены",
                        );
                    }
                    Err(e) => {
                        self.set_status(
                            StatusKind::Error,
                            format!("Не удалось открыть устройство: {}", e),
                        );
                    }
                }
            }
        }
    }

    pub fn plan_polling_with_summary(&mut self) {
        self.plan_polling();
        if let Ok(plan) = crate::service::plan_polling_rate(self.selected_polling) {
            let mut overrides = vec![(plan.bank, plan.data)];
            if let Some(slot) = self.active_slot_override {
                if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(slot) {
                    overrides.push((registers::ACTIVE_DPI_SLOT.0, raw_slot));
                }
            }
            let (all, sample, count) = if let Ok(block) =
                crate::service::resolve_block18(&self.ledger, &overrides)
            {
                let raw_frames = crate::service::plan_block_session_frames(&[block], None, false);
                let count = raw_frames.len();
                let sample = raw_frames.first().map(|f| hex_format(f)).unwrap_or_default();
                let all = raw_frames
                    .iter()
                    .enumerate()
                    .map(|(i, f)| format!("[{:04}] {}", i, hex_format(f)))
                    .collect::<Vec<_>>();
                (all, sample, count)
            } else {
                let all = plan.log_frames();
                let count = all.len();
                let sample = hex_format(&plan.data_frame);
                (all, sample, count)
            };
            self.dry_run_summary = Some(DryRunSummary {
                title: "Частота опроса".to_string(),
                description: format!(
                    "Частота опроса {} Гц (Банк {}, регистр 0x{:04x}, данные 0x{:04x})",
                    self.selected_polling, plan.bank, plan.addr, plan.data
                ),
                frame_count: count,
                sample_frame: sample,
                all_frames: all,
            });
            self.set_status(
                StatusKind::Plan,
                format!(
                    "План: частота опроса {} Гц (кадров: {})",
                    self.selected_polling, count
                ),
            );
        }
    }

    pub fn plan_dpi_full_with_summary(&mut self) {
        self.plan_dpi_full_configuration();
        if let Ok(dpi_plans) = crate::service::plan_dpi_slots(&self.dpi_slots) {
            let colors = self.rgb_colors();
            if let Ok(color_plans) = crate::service::plan_color_session(
                &colors,
                &crate::codec::DEFAULT_SECOND_COLORS,
                &self.ledger,
                Some(self.led_brightness),
            ) {
                let mut all_frames = Vec::new();
                for p in dpi_plans.iter().chain(color_plans.iter()) {
                    for line in p.log_frames() {
                        all_frames.push(line);
                    }
                }
                let sample = dpi_plans
                    .first()
                    .map(|p| hex_format(&p.data_frame))
                    .unwrap_or_default();
                let count = all_frames.len();
                self.dry_run_summary = Some(DryRunSummary {
                    title: "Параметры DPI и цветов".to_string(),
                    description: format!(
                        "6 уровней DPI ({:?}), активный слот {}, яркость {}",
                        self.dpi_slots, self.selected_dpi_slot, self.led_brightness
                    ),
                    frame_count: count,
                    sample_frame: sample,
                    all_frames,
                });
                self.set_status(
                    StatusKind::Plan,
                    format!(
                        "План: DPI и цвета ({} планов, строк кадров: {})",
                        dpi_plans.len() + color_plans.len(),
                        count
                    ),
                );
            }
        }
    }

    pub fn plan_dpi_slot_with_summary(&mut self) {
        self.plan_dpi_slot();
        if let Ok(plan) = crate::service::plan_dpi_slot(self.selected_dpi_slot) {
            let sample = hex_format(&plan.data_frame);
            let all = plan.log_frames();
            let count = all.len();
            self.dry_run_summary = Some(DryRunSummary {
                title: "Смена активного слота DPI".to_string(),
                description: format!(
                    "Активный слот {} (Банк {}, регистр 0x{:04x}, данные 0x{:04x})",
                    self.selected_dpi_slot, plan.bank, plan.addr, plan.data
                ),
                frame_count: count,
                sample_frame: sample,
                all_frames: all,
            });
            self.set_status(
                StatusKind::Plan,
                format!(
                    "План: смена активного слота на {} (кадров: {})",
                    self.selected_dpi_slot, count
                ),
            );
        }
    }

    pub fn plan_fire_delay_with_summary(&mut self) {
        self.plan_fire_delay_action();
        if let Ok((p_low, p_high)) = crate::service::plan_fire_delay(self.fire_delay) {
            let sample = hex_format(&p_low.data_frame);
            let mut all = p_low.log_frames();
            all.extend(p_high.log_frames());
            let count = all.len();
            self.dry_run_summary = Some(DryRunSummary {
                title: "Задержка отдачи".to_string(),
                description: format!(
                    "Задержка кнопки огня: {} мс (Банки 2 и 3, регистры 0x0018)",
                    self.fire_delay
                ),
                frame_count: count,
                sample_frame: sample,
                all_frames: all,
            });
            self.set_status(
                StatusKind::Plan,
                format!("План: задержка отдачи {} мс (кадров: {})", self.fire_delay, count),
            );
        }
    }

    pub fn plan_led_with_summary(&mut self) {
        self.plan_led();
        let opt = &crate::gui::LED_EFFECT_OPTIONS[self.selected_effect_idx];
        let wire_byte = opt.spec.wire_byte(self.led_speed);
        let mask = crate::codec::led::encode_palette_mask(self.all_colors, &self.selected_colors);

        let mut overrides_18 = vec![
            (registers::LED_EFFECT_SPEED.0, 0x0700 | wire_byte as u16),
            (registers::LED_COLOR_PALETTE.0, 0x0700 | mask as u16),
        ];
        if let Some(slot) = self.active_slot_override {
            if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(slot) {
                overrides_18.push((registers::ACTIVE_DPI_SLOT.0, raw_slot));
            }
        } else if !self.ledger.entries.iter().any(|e| e.bank == 0 && e.addr == 0x0018) {
            if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(self.selected_dpi_slot) {
                overrides_18.push((registers::ACTIVE_DPI_SLOT.0, raw_slot));
            }
        }
        overrides_18.push((registers::FIRE_DELAY_LOW.0, 0x0700 | (self.fire_delay & 0xff)));
        overrides_18.push((registers::FIRE_DELAY_HIGH.0, 0x0700 | ((self.fire_delay >> 8) & 0xff)));

        if let (Ok(b18), Ok(b38)) = (
            crate::service::resolve_block18(&self.ledger, &overrides_18),
            crate::service::resolve_block38(&self.ledger, Some(self.led_brightness)),
        ) {
            let session18_raw = crate::service::plan_block_session_frames(&[b18.clone()], None, false);
            let session38_raw = crate::service::plan_block_session_frames(&[b38.clone()], None, false);
            let session18_frames = session18_raw.len();
            let session38_frames = session38_raw.len();
            let total_frames = session18_frames + session38_frames;

            let sample = session18_raw.first().map(|f| hex_format(f)).unwrap_or_default();
            let mut all_frames = Vec::with_capacity(total_frames);
            for (i, f) in session18_raw.iter().chain(session38_raw.iter()).enumerate() {
                all_frames.push(format!("[{:04}] {}", i, hex_format(f)));
            }

            self.pending_plan = None;
            self.pending_blocks = vec![b18, b38.clone()];

            self.dry_run_summary = Some(DryRunSummary {
                title: "Подсветка".to_string(),
                description: format!(
                    "Эффект: '{}' (скорость {}), палитра 0x{:02x}, яркость {} (Bank 7 @ 0x0038: 0x{:04x}) • Сессия 0x0018: {} кадров, Сессия 0x0038: {} кадров",
                    opt.label, self.led_speed, mask, self.led_brightness, b38.banks[7], session18_frames, session38_frames
                ),
                frame_count: total_frames,
                sample_frame: sample,
                all_frames,
            });
            self.set_status(
                StatusKind::Plan,
                format!(
                    "План: подсветка '{}', яркость {} (Bank 7: 0x{:04x}) • Кадров: {} (0x0018: {}, 0x0038: {})",
                    opt.label, self.led_brightness, b38.banks[7], total_frames, session18_frames, session38_frames
                ),
            );
        }
    }

    pub fn plan_restore_with_summary(&mut self) {
        self.plan_restore();
        if let Some(snap) = &self.snapshot_preview {
            let sample = snap.frames.first().map(|f| hex_format(f)).unwrap_or_default();
            let all = snap
                .frames
                .iter()
                .enumerate()
                .map(|(i, f)| format!("[{:04}] {}", i, hex_format(f)))
                .collect();
            self.dry_run_summary = Some(DryRunSummary {
                title: "Восстановление снимка".to_string(),
                description: format!("Восстановление снимка '{}' (PID 0x{:04x})", snap.label, snap.source_pid),
                frame_count: snap.frames.len(),
                sample_frame: sample,
                all_frames: all,
            });
            self.set_status(
                StatusKind::Plan,
                format!("План: восстановление '{}' (кадров: {})", snap.label, snap.frames.len()),
            );
        }
    }
}

impl eframe::App for PanteonApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ensure_theme(ctx);

        // Left Navigation Rail (fixed RAIL_W width = 64px, exactly 4 buttons 44x44)
        egui::SidePanel::left("nav_rail")
            .resizable(false)
            .exact_width(theme::RAIL_W)
            .frame(
                egui::Frame::new()
                    .fill(theme::RAIL_BG)
                    .inner_margin(egui::Margin::symmetric(10, 16))
                    .stroke(egui::Stroke::new(1.0, theme::CARD_STROKE)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                let tabs = [
                    (GuiTab::State, "ℹ", "Состояние"),
                    (GuiTab::Dpi, "⊞", "DPI"),
                    (GuiTab::Lighting, "◎", "Подсветка"),
                    (GuiTab::Profiles, "⚙", "Профили"),
                ];
                let btn_size = egui::vec2(theme::NAV_BTN, theme::NAV_BTN);

                for (tab, icon, label) in tabs {
                    let is_active = self.active_tab == tab;
                    if is_active {
                        let active_frame = egui::Frame::new()
                            .fill(theme::ACCENT)
                            .corner_radius(egui::CornerRadius::same(theme::NAV_RADIUS as u8))
                            .shadow(egui::Shadow {
                                offset: [0, 0],
                                blur: 16,
                                spread: 0,
                                color: theme::ACCENT_GLOW,
                            });
                        active_frame.show(ui, |ui| {
                            let text = egui::RichText::new(icon)
                                .size(18.0)
                                .strong()
                                .color(egui::Color32::WHITE);
                            let btn = egui::Button::new(text)
                                .min_size(btn_size)
                                .fill(egui::Color32::TRANSPARENT)
                                .stroke(egui::Stroke::NONE)
                                .corner_radius(egui::CornerRadius::same(theme::NAV_RADIUS as u8));
                            let resp = theme::tooltip(ui.add(btn), label);
                            if resp.clicked() {
                                self.active_tab = tab;
                            }
                        });
                    } else {
                        let text = egui::RichText::new(icon)
                            .size(18.0)
                            .color(theme::TEXT_SECONDARY);
                        let btn = egui::Button::new(text)
                            .min_size(btn_size)
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE)
                            .corner_radius(egui::CornerRadius::same(theme::NAV_RADIUS as u8));
                        let resp = theme::tooltip(ui.add(btn), label);
                        if resp.clicked() {
                            self.active_tab = tab;
                        }
                    }
                }
            });

        // Fixed Status Bar at the bottom
        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme::RAIL_BG)
                    .stroke(egui::Stroke::new(1.0, theme::CARD_STROKE))
                    .inner_margin(egui::Margin::symmetric(14, 8)),
            )
            .show(ctx, |ui| {
                let (kind, text) = self.current_status();
                // Glyph contract: every icon below must exist in the bundled egui fonts or
                // in the system fallback — otherwise it renders as tofu (▯).
                // NOTE: U+24D8 'ⓘ' has no glyph in Ubuntu-Light, DejaVu Sans nor Noto
                // Symbols 2 (verified), so "plan" uses U+2139 'ℹ' (same meaning, covered).
                // Guarded by `gui_icons_have_glyphs`.
                let icon = match kind {
                    StatusKind::Success => "✓",
                    StatusKind::Error => "✗",
                    StatusKind::Plan => "ℹ",
                    StatusKind::Warning => "⚠",
                };

                let dev_text = if let Some(dev) = self.current_device() {
                    if dev.identity.protocol_verified {
                        "• handshake 0x0976 совпал — протокол подтверждён, запись разрешена"
                    } else {
                        "• протокол не подтверждён, запись заблокирована"
                    }
                } else {
                    "• устройство не выбрано"
                };
                let status_bar_tip = format!("Устройство\n{}\nПоследняя операция\n• {}", dev_text, text);
                let access_chip = self
                    .access_report
                    .as_ref()
                    .filter(|r| !crate::device::report_is_ok(r))
                    .map(access_panel::chip_tooltip);

                let resp = ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(icon).strong().color(kind.color()));
                    ui.label(&text);

                    if let Some(tip) = access_chip {
                        let chip = egui::Button::new(
                            egui::RichText::new(format!("⚠ {}", access_panel::CHIP_TEXT))
                                .small()
                                .color(theme::WARN),
                        )
                        .fill(theme::RAIL_BG)
                        .stroke(egui::Stroke::new(1.0, theme::WARN))
                        .corner_radius(egui::CornerRadius::same(8));
                        let chip = theme::tooltip(ui.add(chip), tip);
                        if chip.clicked() {
                            self.access_modal_open = true;
                        }
                    }

                    if let Some(dev) = self.current_device() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!("PID 0x{:04x}", dev.product_id))
                                    .small()
                                    .color(theme::TEXT_SECONDARY),
                            );
                        });
                    }
                }).response;
                resp.on_hover_ui(|ui| {
                    ui.set_max_width(theme::TOOLTIP_W);
                    ui.label(
                        egui::RichText::new(status_bar_tip)
                            .size(theme::TOOLTIP_TEXT_SIZE)
                            .color(theme::TEXT),
                    );
                });
            });

        // Main Tab Content Area with glass background, gradient and radial glow
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG).inner_margin(egui::Margin::symmetric(16, 12)))
            .show(ctx, |ui| {
                let panel_rect = ui.max_rect();
                let painter = ui.painter_at(panel_rect);
                theme::paint_background_gradient(&painter, panel_rect);
                theme::paint_accent_glow(&painter, panel_rect);

                let dirty = self.dirty_categories();
                let is_dirty = !dirty.is_empty();
                let can_w = crate::gui::check_write_permission(self.current_identity()).is_ok();

                let avail = ui.available_width();
                let content_w = theme::CONTENT_W.min(avail);
                let margin = ((avail - content_w) / 2.0).max(0.0);

                // Fixed non-scrolling Header & CTA row (centered symmetrically with cards)
                ui.horizontal(|ui| {
                    if margin > 0.0 {
                        ui.add_space(margin);
                    }
                    ui.vertical(|ui| {
                        ui.set_width(content_w);
                        ui.horizontal(|ui| {
                            let title_text = match self.active_tab {
                                GuiTab::State => "Состояние устройства",
                                GuiTab::Dpi => "Сенсор и DPI",
                                GuiTab::Lighting => "Подсветка",
                                GuiTab::Profiles => "Профили и снимки",
                            };
                            ui.label(egui::RichText::new(title_text).strong().size(20.0).color(theme::TEXT));

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if is_dirty {
                                    let apply_frame = egui::Frame::new()
                                        .shadow(egui::Shadow {
                                            offset: [0, 0],
                                            blur: 16,
                                            spread: 0,
                                            color: theme::ACCENT_GLOW,
                                        })
                                        .corner_radius(egui::CornerRadius::same(theme::NAV_RADIUS as u8));

                                    apply_frame.show(ui, |ui| {
                                        let cta = egui::Button::new(
                                            egui::RichText::new(format!("Применить ({})", dirty.len()))
                                                .strong()
                                                .size(13.0)
                                                .color(egui::Color32::WHITE),
                                        )
                                        .min_size(egui::vec2(140.0, theme::PILL_H))
                                        .corner_radius(egui::CornerRadius::same(theme::NAV_RADIUS as u8))
                                        .fill(theme::ACCENT);

                                        let mut cta_resp = ui.add_enabled(can_w, cta);
                                        if can_w {
                                            cta_resp = theme::tooltip(cta_resp, "Применить все изменения\n• одна сессия записи\n• потребуется подтверждение");
                                        } else {
                                            cta_resp = theme::disabled_tooltip(cta_resp, "Запись недоступна\n• нет прав уровня 2");
                                        }
                                        if cta_resp.clicked() {
                                            self.request_cumulative_apply();
                                        }
                                    });

                                    if theme::quiet_pill_button(
                                        ui,
                                        "discard_btn",
                                        "Сбросить",
                                        "Отменить несохранённые правки\n• вернуть панель к последнему применённому состоянию",
                                    )
                                    .clicked()
                                    {
                                        self.discard_changes();
                                    }

                                    if theme::quiet_pill_button(
                                        ui,
                                        "diff_btn",
                                        "Изменения",
                                        "Просмотреть изменения\n• список всех параметров, отличающихся от сохранённых в журнале",
                                    )
                                    .clicked()
                                    {
                                        self.diff_dialog_open = true;
                                    }
                                }
                            });
                        });
                    });
                });

                ui.add_space(8.0);

                ui.scope(|ui| {
                    ui.style_mut().visuals.extreme_bg_color = theme::SEPARATOR;
                    ui.style_mut().visuals.widgets.inactive.bg_fill = theme::ACCENT;
                    ui.style_mut().visuals.widgets.hovered.bg_fill = theme::ACCENT;
                    ui.style_mut().visuals.widgets.active.bg_fill = theme::ACCENT;
                    ui.style_mut().spacing.scroll = egui::style::ScrollStyle {
                        floating: true,
                        bar_width: 6.0,
                        handle_min_length: 32.0,
                        bar_inner_margin: 0.0,
                        bar_outer_margin: 4.0,
                        floating_width: 6.0,
                        floating_allocated_width: 0.0,
                        foreground_color: false,
                        dormant_background_opacity: 0.3,
                        active_background_opacity: 0.3,
                        interact_background_opacity: 0.4,
                        dormant_handle_opacity: 0.35,
                        active_handle_opacity: 0.35,
                        interact_handle_opacity: 0.7,
                    };

                    egui::ScrollArea::vertical()
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            ui.style_mut().visuals.extreme_bg_color = theme::RAIL_BG;
                            ui.style_mut().visuals.widgets.inactive.bg_fill = theme::HOVER_BG;
                            ui.style_mut().visuals.widgets.hovered.bg_fill = theme::HOVER_BG;
                            ui.style_mut().visuals.widgets.active.bg_fill = theme::ACCENT;

                            let avail = ui.available_width();
                            let content_w = theme::CONTENT_W.min(avail);
                            let margin = ((avail - content_w) / 2.0).max(0.0);

                            ui.horizontal(|ui| {
                                if margin > 0.0 {
                                    ui.add_space(margin);
                                }
                                ui.vertical(|ui| {
                                    ui.set_width(content_w);
                                    match self.active_tab {
                                        GuiTab::State => render_state_tab(ui, self),
                                        GuiTab::Dpi => render_dpi_tab(ui, self),
                                        GuiTab::Lighting => render_lighting_tab(ui, self),
                                        GuiTab::Profiles => render_profiles_tab(ui, self),
                                    }
                                });
                            });
                        });
                });
            });

        render_diff_dialog(ctx, self);
        render_confirm_dialog(ctx, self);
        crate::gui::render_preset_modals(ctx, self);
        access_panel::render_access_modal(ctx, self);
    }
}

/// Applies the Apple-style dark theme and the symbol fallback font **once per session**.
/// Per-frame `set_style` + `set_fonts` used to run on every frame; `set_fonts` re-reads the
/// TTF from disk and deep-compares the font data (megabytes) each frame — pure latency in
/// the input loop, which also made tooltips (which require a still pointer for the whole
/// delay window) feel dead. Style/fonts are session-level state: install them once.
fn ensure_theme(ctx: &egui::Context) {
    let installed = egui::Id::new("panteon_theme_installed");
    if ctx.data(|d| d.get_temp::<bool>(installed)).unwrap_or(false) {
        return;
    }
    apply_apple_dark_theme(ctx);
    ctx.data_mut(|d| d.insert_temp(installed, true));
}

/// Apple-style dark theme (dark mode system grays + system accent colors).
/// Light backgrounds stay reserved for the confirm modal (destructive action = elevated surface).
pub fn apply_apple_dark_theme(ctx: &egui::Context) {
    use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Vec2};

    let bg = theme::BG;
    let elevated = theme::CARD;
    let field = theme::RAIL_BG;
    let accent = theme::ACCENT;
    let label = theme::TEXT;
    let separator = theme::SEPARATOR;

    let mut style = (*ctx.style()).clone();

    style.visuals.dark_mode = true;
    style.visuals.panel_fill = bg;
    // Elevated surfaces (modal windows, ComboBox popups) must be opaque: they
    // cover live content (cards with text), which used to bleed through.
    style.visuals.window_fill = theme::CARD_OPAQUE;
    style.visuals.extreme_bg_color = field;
    style.visuals.window_stroke = Stroke::new(1.0, separator);
    style.visuals.window_corner_radius = CornerRadius::same(theme::CARD_R as u8);
    style.visuals.menu_corner_radius = CornerRadius::same(8);
    style.visuals.selection.bg_fill = accent.gamma_multiply(0.35);
    style.visuals.selection.stroke = Stroke::new(1.0, accent);
    style.visuals.slider_trailing_fill = true;
    style.visuals.handle_shape = egui::style::HandleShape::Circle;
    style.visuals.hyperlink_color = accent;
    style.visuals.override_text_color = Some(label);
    style.visuals.widgets.noninteractive.bg_fill = bg;
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, label);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, separator);
    style.visuals.widgets.inactive.bg_fill = theme::HOVER_BG;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, label);
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
    style.visuals.widgets.hovered.bg_fill = theme::HOVER_BG;
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
    style.visuals.widgets.active.bg_fill = accent;
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(6);
    style.visuals.widgets.open.bg_fill = elevated;
    style.visuals.widgets.open.corner_radius = CornerRadius::same(6);

    style.spacing.scroll = egui::style::ScrollStyle {
        floating: true,
        bar_width: 6.0,
        handle_min_length: 32.0,
        bar_inner_margin: 0.0,
        bar_outer_margin: 4.0,
        floating_width: 6.0,
        floating_allocated_width: 0.0,
        foreground_color: false,
        dormant_background_opacity: 0.3,
        active_background_opacity: 0.3,
        interact_background_opacity: 0.4,
        dormant_handle_opacity: 0.35,
        active_handle_opacity: 0.35,
        interact_handle_opacity: 0.7,
    };
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.button_padding = Vec2::new(14.0, 7.0);
    style.spacing.menu_margin = Margin::same(6);
    style.spacing.interact_size = Vec2::new(40.0, 30.0);
    style.spacing.indent = 20.0;

    // Tooltips: narrow wrapped column (never wider than the window), fast reaction,
    // dark CARD surface with 8px radius (menu_corner_radius) and 12.5px text.
    style.spacing.tooltip_width = theme::TOOLTIP_W;
    style.interaction.tooltip_delay = theme::TOOLTIP_DELAY;
    style.interaction.tooltip_grace_time = theme::TOOLTIP_GRACE;

    let heading = FontId::new(20.0, FontFamily::Proportional);
    let body = FontId::new(14.0, FontFamily::Proportional);
    let small = FontId::new(12.0, FontFamily::Proportional);
    style.text_styles = [
        (TextStyle::Heading, heading),
        (TextStyle::Body, body.clone()),
        (TextStyle::Button, body.clone()),
        (TextStyle::Small, small),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into_iter()
    .collect();

    ctx.set_style(style);
    install_symbol_fallback_font(ctx);
}

/// egui's bundled Ubuntu-Light subset lacks check/warning glyphs. Register every
/// available system font as an *additional* fallback (DejaVu first, then Noto
/// Symbols, then Liberation) so the GUI icon set has full coverage.
/// Previously only the first found file was installed and the loop returned, so
/// symbols living in Noto Symbols (e.g. U+24D8 'ⓘ') were left as tofu.
fn install_symbol_fallback_font(ctx: &egui::Context) {
    let candidates = [
        ("symbol_fallback_dejavu", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        (
            "symbol_fallback_noto",
            "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
        ),
        (
            "symbol_fallback_liberation",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        ),
    ];

    let mut fonts = egui::FontDefinitions::default();
    let mut installed = 0usize;
    for (name, path) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                name.to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push(name.to_owned());
            }
            installed += 1;
        }
    }
    if installed > 0 {
        ctx.set_fonts(fonts);
    }
}

/// Fire delay from the Bank 2 (low) / Bank 3 (high) register values.
/// Inverse of the `0x0700 | delay_byte` encoding used by `service::plan_fire_delay`.
fn fire_delay_from_registers(low: u16, high: u16) -> u16 {
    ((high & 0x00ff) << 8) | (low & 0x00ff)
}

/// Formats a unix timestamp (seconds) as a compact local-time string without external crates.
/// Days-to-civil algorithm (Howard Hinnant's date algorithms).
pub fn format_unix_ts(ts: u64) -> String {
    let days = (ts as i64).div_euclid(86_400);
    let secs = (ts as i64).rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y,
        m,
        d,
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

/// Days since 1970-01-01 -> (year, month, day). Howard Hinnant's civil_from_days.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Short Russian month names as used in "21 ноя, 02:06".
const MONTHS_SHORT_RU: [&str; 12] = [
    "янв", "фев", "мар", "апр", "мая", "июн", "июл", "авг", "сен", "окт", "ноя", "дек",
];

/// Human timestamp relative to `now` (both unix seconds), no seconds shown:
/// today -> "сегодня, 02:06", yesterday -> "вчера, 16:32", otherwise -> "21 ноя, 02:06".
/// Pure function (no egui, no clock access) so it is unit-testable at date boundaries.
pub fn format_human_ts(ts: u64, now: u64) -> String {
    let ts = ts as i64;
    let now = now as i64;
    let day = ts.div_euclid(86_400);
    let today = now.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let hm = format!("{:02}:{:02}", secs / 3600, (secs % 3600) / 60);

    match today - day {
        0 => format!("сегодня, {}", hm),
        1 => format!("вчера, {}", hm),
        _ => {
            let (_, month, day_of_month) = civil_from_days(day);
            format!(
                "{} {}, {}",
                day_of_month,
                MONTHS_SHORT_RU[(month - 1) as usize],
                hm
            )
        }
    }
}

pub fn run_gui() -> eframe::Result<()> {
    let width = std::env::var("PANTEON_WIDTH")
        .ok()
        .and_then(|w| w.parse::<f32>().ok())
        .unwrap_or(940.0);
    let height = std::env::var("PANTEON_HEIGHT")
        .ok()
        .and_then(|h| h.parse::<f32>().ok())
        .unwrap_or(900.0);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([width, height])
            .with_min_inner_size([760.0, 600.0])
            .with_title("PANTEON MS89 Configurator"),
        ..Default::default()
    };
    eframe::run_native(
        "panteon-ms89-gui",
        native_options,
        Box::new(|cc| Ok(Box::new(PanteonApp::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400;

    fn at(day: u64, h: u64, m: u64) -> u64 {
        day * DAY + h * 3600 + m * 60
    }

    #[test]
    fn human_ts_today() {
        // 2024-11-21, now 12:00, write at 02:06 the same day
        let day = 20_048;
        assert_eq!(
            format_human_ts(at(day, 2, 6), at(day, 12, 0)),
            "сегодня, 02:06"
        );
    }

    #[test]
    fn human_ts_yesterday() {
        let day = 20_048;
        assert_eq!(
            format_human_ts(at(day - 1, 16, 32), at(day, 0, 30)),
            "вчера, 16:32"
        );
    }

    #[test]
    fn human_ts_midnight_boundary() {
        let day = 20_048;
        // 23:59:59 of the previous day is still "вчера", not "сегодня"
        let last_second = (day - 1) * DAY + DAY - 1;
        assert_eq!(format_human_ts(last_second, day * DAY), "вчера, 23:59");
        // exactly at midnight of today -> "сегодня, 00:00"
        assert_eq!(format_human_ts(day * DAY, day * DAY), "сегодня, 00:00");
    }

    #[test]
    fn human_ts_month_rollover() {
        // now = 2024-03-01 09:00 (day 19783); yesterday was 2024-02-29 (leap year)
        let today = 19_783;
        assert_eq!(
            format_human_ts(at(today - 1, 22, 0), at(today, 9, 0)),
            "вчера, 22:00"
        );
        // 2024-02-27 is "27 фев" (no seconds printed)
        assert_eq!(
            format_human_ts(at(today - 3, 10, 5), at(today, 9, 0)),
            "27 фев, 10:05"
        );
    }

    #[test]
    fn human_ts_arbitrary_date() {
        // 2024-11-21 02:06, "now" ~10 days later -> absolute date, month abbreviated
        let written = 20_048;
        assert_eq!(
            format_human_ts(at(written, 2, 6), at(written + 10, 12, 0)),
            "21 ноя, 02:06"
        );
    }

    #[test]
    fn human_ts_epoch_zero() {
        // ts = 0 is 1970-01-01 00:00 (and is "сегодня" when now is also 0)
        assert_eq!(format_human_ts(0, 0), "сегодня, 00:00");
        assert_eq!(format_human_ts(0, 20_048 * DAY), "1 янв, 00:00");
    }

    #[test]
    fn human_ts_has_no_seconds() {
        let s = format_human_ts(at(20_048, 2, 6), at(20_049, 0, 0));
        assert_eq!(s.matches(':').count(), 1, "no seconds in human timestamp: {s}");
    }

    /// Glyph contract: every icon the GUI paints must resolve to a real glyph in the
    /// theme's font stack. A missing glyph renders as tofu (▯) in the status bar /
    /// nav rail — exactly the `ⓘ` (U+24D8) regression this test pins down.
    #[test]
    fn gui_icons_have_glyphs() {
        let ctx = egui::Context::default();
        apply_apple_dark_theme(&ctx);
        // Fonts are swapped at the beginning of a pass, so run one frame.
        let _ = ctx.run(Default::default(), |_| {});

        let icons = [
            // status bar (StatusKind)
            '\u{2713}', // ✓ success
            '\u{2717}', // ✗ error
            '\u{2139}', // ℹ plan
            '\u{26A0}', // ⚠ warning
            // nav rail
            '\u{229E}', // ⊞ DPI
            '\u{25CE}', // ◎ lighting
            '\u{2699}', // ⚙ profiles
            // other surfaces (preset list, separators, hints)
            '\u{25B8}', // ▸ preset
            '\u{2212}', // − delete
            '\u{00B7}', // · separator
            '\u{2014}', // — em dash
            '\u{2026}', // … ellipsis
            '\u{2022}', // • bullet
            '\u{2039}', // ‹
            '\u{203A}', // ›
            '\u{2715}', // ✕ preset delete (toolbar)
            // typographic quotes used by the R16 preset dialogs
            '\u{201E}', // „
            '\u{201C}', // “
            '\u{00AB}', // «
            '\u{00BB}', // »
        ];

        let font_id = egui::FontId::new(14.0, egui::FontFamily::Proportional);
        let missing: Vec<String> = icons
            .iter()
            .filter(|c| !ctx.fonts(|f| f.has_glyph(&font_id, **c)))
            .map(|c| format!("U+{:04X} {:?}", *c as u32, c))
            .collect();
        assert!(
            missing.is_empty(),
            "GUI icons without a glyph would render as tofu: {missing:?}"
        );
    }

    /// Synthetic ledger mirroring a real capture (bank/addr values from the
    /// user's `ledger.toml`): polling 1000 Hz, effect «6. Комета» speed 2,
    /// all colours, fire delay 50 ms, brightness 2, active slot 4.
    fn ledger_with_device_state() -> ShadowLedger {
        let mut ledger = ShadowLedger::default();
        for (bank, addr, data) in [
            (0u8, 0x0018u16, 0x071bu16),
            (3, 0x0010, 0x0720),
            (5, 0x0010, 0x0712),
            (6, 0x0010, 0x0749),
            (7, 0x0010, 0x07d1),
            (6, 0x0018, 0x0700),
            (4, 0x0018, 0x07d2),
            (5, 0x0018, 0x07ff),
            (2, 0x0018, 0x0732),
            (3, 0x0018, 0x0700),
            (7, 0x0038, 0x0760),
        ] {
            ledger.append(crate::model::ShadowEntry::new(bank, addr, data, "1700000000"));
        }
        ledger
    }

    #[test]
    fn project_editor_from_ledger_covers_all_projected_fields() {
        let mut app = PanteonApp::default();
        app.ledger = ledger_with_device_state();
        app.project_editor_from_ledger();
        app.sync_baseline();

        assert_eq!(app.selected_polling, 1000);
        assert_eq!(app.selected_effect_idx, 5, "bank 4 @0x18 = 0x07d2 -> «6. Комета»");
        assert_eq!(
            crate::gui::LED_EFFECT_OPTIONS[app.selected_effect_idx].label,
            "6. Комета",
            "0x07d2 must project onto the «6. Комета» catalog entry"
        );
        assert_eq!(app.led_speed, 2);
        assert!(app.all_colors);
        assert_eq!(app.selected_colors, [false; 7]);
        assert_eq!(app.fire_delay, 50);
        assert_eq!(app.led_brightness, 2);
        assert_eq!(app.selected_dpi_slot, 4);
        assert_eq!(app.dpi_slots, [200, 400, 800, 1800, 3200, 6400]);
        assert_eq!(app.baseline.selected_effect_idx, 5);
        assert_eq!(app.baseline.led_speed, 2);
    }

    /// Regression for the reported defect: «create a profile, switch back to the
    /// previous one without changing anything -> «Применить» stays lit».
    /// Loading a profile that equals the ledger must leave the editor clean.
    #[test]
    fn loading_profile_that_matches_ledger_is_not_dirty() {
        let mut app = PanteonApp::default();
        app.ledger = ledger_with_device_state();
        app.project_editor_from_ledger();
        app.sync_baseline();
        assert!(!app.is_dirty());

        // «создаю новый профиль» — snapshot of the current (device) state
        let profile = crate::gui::preset_panel::build_profile_from_app(&app);

        // «переключаюсь на прежний» — load that profile back, ничего не меняя
        crate::gui::preset_panel::load_profile_into_app(&mut app, &profile);

        assert!(
            !app.is_dirty(),
            "loading a profile that matches the ledger must not light up «Применить»: {:?}",
            app.diff_entries()
        );
        assert!(app.dirty_categories().is_empty());

        // R20: the legacy «level 0» wire encoding (nibble 8, 0x0780) is the same
        // physical state as 100%. Projection and baseline canonicalize it to 8,
        // and a profile carrying wire 0 loads as 8 — still not dirty (no false
        // «profile modified» on the R18 indicator).
        app.ledger
            .append(crate::model::ShadowEntry::new(7, 0x0038, 0x0780, "1700000001"));
        app.project_editor_from_ledger();
        app.sync_baseline();
        assert_eq!(app.led_brightness, 8, "nibble 8 must normalize to 100%");
        assert_eq!(app.baseline.led_brightness, 8);
        assert!(
            !app.diff_entries().iter().any(|d| d.0 == "Яркость LED"),
            "nibble 8 and level 8 are the same brightness — no diff row"
        );

        let mut legacy = crate::gui::preset_panel::build_profile_from_app(&app);
        legacy.led_brightness = 0;
        crate::gui::preset_panel::load_profile_into_app(&mut app, &legacy);
        assert_eq!(app.led_brightness, 8, "profile wire 0 must load as 100%");
        assert!(
            !app.is_dirty(),
            "wire 0 profile matching a nibble-8 ledger must stay clean: {:?}",
            app.diff_entries()
        );
    }

    /// R20: wire level 1 (nibble 7, 0x0770) is the real «off»; the diff row must
    /// use the semantic percent (0.0%), not a naive linear level scale.
    #[test]
    fn ledger_brightness_nibble7_is_off_and_diff_uses_semantic_percent() {
        let mut app = PanteonApp::default();
        app.ledger = ledger_with_device_state();
        app.ledger
            .append(crate::model::ShadowEntry::new(7, 0x0038, 0x0770, "1700000001"));
        app.project_editor_from_ledger();
        app.sync_baseline();

        assert_eq!(app.led_brightness, 1, "nibble 7 must decode to wire level 1");
        assert_eq!(app.baseline.led_brightness, 1);
        assert_eq!(crate::codec::brightness_percent(1), 0.0);
        assert!(!app.is_dirty());

        app.led_brightness = 8;
        let diffs = app.diff_entries();
        let row = diffs
            .iter()
            .find(|d| d.0 == "Яркость LED")
            .expect("brightness change must be listed");
        assert_eq!(row.1, "1 (0.0%)");
        assert_eq!(row.2, "8 (100.0%)");
    }

    /// Guard against over-correcting: a real difference must still be reported.
    #[test]
    fn real_difference_is_still_dirty() {
        let mut app = PanteonApp::default();
        app.ledger = ledger_with_device_state();
        app.project_editor_from_ledger();
        app.sync_baseline();
        assert!(!app.is_dirty());

        app.led_speed = 4;
        assert!(app.is_dirty());
        assert!(app.dirty_categories().contains(&DirtyCategory::Led));

        app.led_speed = 2;
        app.selected_polling = 250;
        assert!(app.dirty_categories().contains(&DirtyCategory::Polling));

        // And the fire delay helper used by the projection.
        assert_eq!(fire_delay_from_registers(0x0732, 0x0700), 50);
        assert_eq!(fire_delay_from_registers(0x0708, 0x0700), 8);
        assert_eq!(fire_delay_from_registers(0x0700, 0x0701), 256);
    }
}
