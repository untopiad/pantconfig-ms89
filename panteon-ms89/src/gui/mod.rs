pub mod access_panel;
pub mod actions;
pub mod app;
pub mod dpi_panel;
pub mod expected_state;
pub mod panels;
pub mod preset_panel;
pub mod snapshot_panel;
pub mod theme;

pub use access_panel::*;
pub use app::{
    run_gui, AppBaseline, AppStatus, DirtyCategory, DryRunSummary, GuiTab, PanteonApp, StatusKind,
};
pub use dpi_panel::*;
pub use expected_state::*;
pub use preset_panel::*;
pub use snapshot_panel::*;

use crate::model::DeviceIdentity;

/// Palette colors in the 7-color GUI palette, indexed by wire bit 0..=6
/// (Bank 5 @ 0x0018 mask). Source of truth: OEM default palette order
/// (verified against live captures and user observation). No ordinal
/// suffixes: the order is the wire bit order.
///
/// Wire statuses per bit: 0/3/6 CONFIRMED_TRACE, 1 USER_LIVE_R20,
/// 2/4/5 CONFIRMED_STATIC_OEM (config + live captures).
pub const PALETTE_COLOR_NAMES: [&str; 7] = [
    "Жёлтый",
    "Синий",
    "Розовый",
    "Зелёный",
    "Красный",
    "Голубой",
    "Фиолетовый",
];

/// 14 LED Effect dropdown items matching OEM utility.
///
/// Презентация: label + ссылка на wire-спеку в [`crate::codec::led`]. Собственных
/// wire-значений у GUI нет (AGENTS §4): `base_byte`, `wire_byte`, `default_speed`
/// и `supports_*` живут в `codec::led::LedModeSpec`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedEffectOption {
    pub label: &'static str,
    pub spec: &'static crate::codec::led::LedModeSpec,
}

pub const LED_EFFECT_OPTIONS: [LedEffectOption; 14] = [
    LedEffectOption { label: "1. DPI дыхание", spec: &crate::codec::led::LED_MODE_SPECS[0] },
    LedEffectOption { label: "2. Дыхание «цикл»", spec: &crate::codec::led::LED_MODE_SPECS[1] },
    LedEffectOption { label: "3. Постоянно", spec: &crate::codec::led::LED_MODE_SPECS[2] },
    LedEffectOption { label: "4. Течение", spec: &crate::codec::led::LED_MODE_SPECS[3] },
    LedEffectOption { label: "5. Течение (один цвет)", spec: &crate::codec::led::LED_MODE_SPECS[4] },
    LedEffectOption { label: "6. Комета", spec: &crate::codec::led::LED_MODE_SPECS[5] },
    LedEffectOption { label: "7. Неон", spec: &crate::codec::led::LED_MODE_SPECS[6] },
    LedEffectOption { label: "8. Амбилайт", spec: &crate::codec::led::LED_MODE_SPECS[7] },
    LedEffectOption { label: "9. Вспышки", spec: &crate::codec::led::LED_MODE_SPECS[8] },
    LedEffectOption { label: "10. Стартрек", spec: &crate::codec::led::LED_MODE_SPECS[9] },
    LedEffectOption { label: "11. Рябь", spec: &crate::codec::led::LED_MODE_SPECS[10] },
    LedEffectOption { label: "12. Взлет", spec: &crate::codec::led::LED_MODE_SPECS[11] },
    LedEffectOption { label: "13. Отклик", spec: &crate::codec::led::LED_MODE_SPECS[12] },
    LedEffectOption { label: "14. Выключено", spec: &crate::codec::led::LED_MODE_SPECS[13] },
];

/// Check if write is permitted for a given device identity
pub fn check_write_permission(identity: Option<&DeviceIdentity>) -> Result<(), &'static str> {
    match identity {
        None => Err("No target device connected or selected"),
        Some(id) if !id.protocol_verified => Err(
            "Write blocked: Protocol is unverified for this PID (Level 2 gate closed). run verify-protocol или --allow-unverified-device в CLI.",
        ),
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Каталог — только презентация: 14 записей, mode_index уникальны и покрывают
    /// 0..=13, каждая spec указывает на элемент LED_MODE_SPECS с тем же индексом.
    #[test]
    fn led_catalog_matches_codec_spec_table() {
        use crate::codec::led::LED_MODE_SPECS;

        assert_eq!(LED_EFFECT_OPTIONS.len(), 14);
        assert_eq!(LED_MODE_SPECS.len(), 14);

        let mut seen = [false; 14];
        for (i, opt) in LED_EFFECT_OPTIONS.iter().enumerate() {
            assert!(!opt.label.trim().is_empty(), "catalog entry {i} has an empty label");
            assert!(
                std::ptr::eq(opt.spec, &LED_MODE_SPECS[i]),
                "catalog entry {i} must point at LED_MODE_SPECS[{i}]"
            );
            let mode_index = opt.spec.mode_index as usize;
            assert!(mode_index < 14, "catalog entry {i} mode_index out of range");
            assert!(!seen[mode_index], "duplicate mode_index {mode_index}");
            seen[mode_index] = true;
        }
        assert!(seen.iter().all(|s| *s), "mode_index must cover 0..=13");
    }

    /// R20 contract: factual OEM palette names, in wire-bit order (live-capture verified).
    /// No prototype ordinal suffixes and no guessed colours.
    #[test]
    fn palette_color_names_match_oem_table() {
        assert_eq!(PALETTE_COLOR_NAMES.len(), 7, "palette has 7 wire bits");
        assert_eq!(
            PALETTE_COLOR_NAMES,
            [
                "Жёлтый",
                "Синий",
                "Розовый",
                "Зелёный",
                "Красный",
                "Голубой",
                "Фиолетовый",
            ],
            "order must match the live-capture OEM table (bits 0..=6)"
        );

        let mut unique = PALETTE_COLOR_NAMES.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 7, "palette names must be unique");

        for name in PALETTE_COLOR_NAMES {
            assert!(!name.trim().is_empty(), "palette name must not be empty");
            assert!(
                !name.contains('(') && !name.contains(')'),
                "ordinal suffixes are forbidden: {name}"
            );
            assert!(
                name.chars().all(|c| c.is_alphabetic()),
                "palette name must be a plain word (Cyrillic): {name}"
            );
        }
    }

    /// Декодированная семантика -> ровно одна запись каталога (без допущения
    /// «mode_index == индекс dropdown»).
    #[test]
    fn led_catalog_lookup_maps_every_mode_index() {
        for spec in crate::codec::led::LED_MODE_SPECS.iter() {
            let matches: Vec<usize> = LED_EFFECT_OPTIONS
                .iter()
                .enumerate()
                .filter(|(_, o)| o.spec.mode_index == spec.mode_index)
                .map(|(i, _)| i)
                .collect();
            assert_eq!(
                matches.len(),
                1,
                "mode_index {} must resolve to exactly one catalog entry",
                spec.mode_index
            );
            assert_eq!(
                LED_EFFECT_OPTIONS[matches[0]].spec.mode_index,
                spec.mode_index,
                "lookup must return the entry carrying the same mode_index"
            );
        }
    }

    #[test]
    fn test_unverified_identity_write_gate_rejection() {
        // No device selected
        assert!(check_write_permission(None).is_err());

        // Unverified PID 0x1e01
        let unverified_id = DeviceIdentity {
            pid: 0x1e01,
            protocol_verified: false,
            accepted_signatures: &[0x0976, 0x0978],
        };
        let err = check_write_permission(Some(&unverified_id)).unwrap_err();
        assert!(err.contains("Write blocked"));
        assert!(err.contains("run verify-protocol или --allow-unverified-device в CLI"));

        // Verified PID 0x1440
        let verified_id = DeviceIdentity {
            pid: 0x1440,
            protocol_verified: true,
            accepted_signatures: &[0x0976, 0x0978],
        };
        assert!(check_write_permission(Some(&verified_id)).is_ok());
    }

    #[test]
    fn test_dpi_slot_planning_and_color_expansion() {
        let slots = crate::codec::DEFAULT_DPI_SLOTS;
        let plans = crate::service::plan_dpi_slots(&slots).unwrap();
        assert_eq!(plans.len(), 8);
        let mut ledger = crate::model::ShadowLedger::default();
        ledger.append(crate::model::ShadowEntry::new(7, 0x0038, 0x0700, "1000"));
        let color_plans = crate::service::plan_color_session(
            &crate::codec::DEFAULT_SLOT_COLORS,
            &crate::codec::DEFAULT_SECOND_COLORS,
            &ledger,
            None,
        )
        .unwrap();
        assert_eq!(color_plans.len(), 32);
    }

    #[test]
    fn test_gui_tab_and_status_enum() {
        let mut app = PanteonApp::default();
        assert_eq!(app.active_tab, GuiTab::State);

        app.active_tab = GuiTab::Dpi;
        assert_eq!(app.active_tab, GuiTab::Dpi);
        app.active_tab = GuiTab::Lighting;
        assert_eq!(app.active_tab, GuiTab::Lighting);
        app.active_tab = GuiTab::Profiles;
        assert_eq!(app.active_tab, GuiTab::Profiles);

        // StatusKind labels
        assert_eq!(StatusKind::Success.label(), "Успех");
        assert_eq!(StatusKind::Plan.label(), "План");
        assert_eq!(StatusKind::Error.label(), "Ошибка");
        assert_eq!(StatusKind::Warning.label(), "Предупреждение");

        // set_status and current_status roundtrip
        app.set_status(StatusKind::Plan, "Тестовый план");
        let (kind, text) = app.current_status();
        assert_eq!(kind, StatusKind::Plan);
        assert_eq!(text, "Тестовый план");

        app.set_status(StatusKind::Error, "Ошибка записи");
        let (kind, text) = app.current_status();
        assert_eq!(kind, StatusKind::Error);
        assert_eq!(text, "Ошибка записи");

        // Compatibility with actions setting status_message directly
        app.status_message = "[Dry-run Plan] Polling rate 1000 Hz".to_string();
        let (kind, text) = app.current_status();
        assert_eq!(kind, StatusKind::Plan);
        assert!(text.contains("1000 Hz"));
    }

    #[test]
    fn test_dry_run_summary_generation() {
        let mut app = PanteonApp::default();
        app.ledger.append(crate::model::ShadowEntry::new(0, 0x0018, 0x071b, "1000"));

        // Polling dry-run summary
        app.plan_polling_with_summary();
        let summary = app.dry_run_summary.as_ref().expect("Polling summary must be generated");
        assert!(summary.frame_count > 0);
        assert!(!summary.sample_frame.is_empty());
        assert!(!summary.all_frames.is_empty());
        assert!(summary.description.contains("1000"));

        // LED dry-run summary
        app.plan_led_with_summary();
        let summary = app.dry_run_summary.as_ref().expect("LED summary must be generated");
        assert!(summary.frame_count > 0);
        assert!(!summary.sample_frame.is_empty());
        assert!(!summary.all_frames.is_empty());

        // Fire delay dry-run summary
        app.plan_fire_delay_with_summary();
        let summary = app.dry_run_summary.as_ref().expect("Fire delay summary must be generated");
        assert!(summary.frame_count > 0);
        assert!(!summary.sample_frame.is_empty());
        assert!(!summary.all_frames.is_empty());
        assert!(summary.description.contains("50"));
    }

    #[test]
    fn slider_is_unconditional() {
        let mut app = PanteonApp::default();
        // Mode 0 is "1. DPI дыхание", which has supports_brightness = false
        app.selected_effect_idx = 0;
        assert!(!LED_EFFECT_OPTIONS[0].spec.supports_brightness);

        // In R10, brightness slider is global (Bank 7 Addr 0x0038) and unconditional
        app.led_brightness = 2;
        assert_eq!(app.led_brightness, 2);
        assert!(app.dirty_categories().contains(&DirtyCategory::Led));
    }

    #[test]
    fn full_push_uses_editor_brightness() {
        let mut app = PanteonApp::default();
        // Put baseline brightness 8 into ledger (0x0700)
        app.ledger.append(crate::model::ShadowEntry::new(7, 0x0038, 0x0700, "1000"));

        // User sets editor brightness to 2
        // R8 active-low encoding: (8 - 2) << 4 = 6 << 4 = 0x60 -> 0x0760
        app.led_brightness = 2;

        let snap = app
            .build_editor_full_push_snapshot()
            .expect("full push snapshot");
        let b7_frame = snap
            .frames
            .iter()
            .find(|f| {
                f[0] == 0x07
                    && f[1] == 0x18
                    && f[2] == 0x03
                    && f[3] == 7
                    && f[4] == 0x38
                    && f[5] == 0x00
            })
            .expect("Bank 7 Addr 0x0038 frame must exist in full push snapshot");

        let data = u16::from_le_bytes([b7_frame[6], b7_frame[7]]);
        assert_eq!(
            data, 0x0760,
            "Full push must reflect editor brightness 2 (expected 0x0760, got 0x{:04x})",
            data
        );
    }

    #[test]
    fn cumulative_apply_two_block_groups() {
        let mut app = PanteonApp::default();
        app.ledger
            .append(crate::model::ShadowEntry::new(0, 0x0018, 0x0718, "1000"));

        // Change category 1: Polling (part of 0x0018 group)
        app.selected_polling = 500;
        // Change category 2: DPI slot (part of 0x30/0x38 group)
        app.dpi_slots[0] = 800;

        let dirty = app.dirty_categories();
        assert!(dirty.contains(&DirtyCategory::Polling));
        assert!(dirty.contains(&DirtyCategory::Dpi));

        let (block_18, blocks_dpi_color) = app
            .plan_cumulative_block_groups()
            .expect("cumulative block groups planning");

        // Group 1: block-session 0x0018
        assert!(block_18.is_some(), "Must produce 0x0018 block group");
        let b18 = block_18.unwrap();
        assert_eq!(b18.addr, 0x0018);
        assert_eq!(b18.banks[6], 0x0701, "Bank 6 must be 500 Hz (0x0701)");

        // Group 2: block-session 0x30/0x38
        assert!(
            !blocks_dpi_color.is_empty(),
            "Must produce 0x30/0x38 DPI and color block group"
        );
        assert!(
            blocks_dpi_color
                .iter()
                .any(|b| b.addr == 0x0030 || b.addr == 0x0038),
            "Must contain 0x30/0x38 blocks"
        );
    }

    #[test]
    fn dirty_flag_reset_after_write() {
        let mut app = PanteonApp::default();
        assert!(!app.is_dirty());
        assert_eq!(app.dirty_categories().len(), 0);

        // Edit multiple categories
        app.selected_polling = 250;
        app.fire_delay = 120;
        app.led_brightness = 6;
        assert!(app.is_dirty());
        assert!(app.dirty_categories().len() >= 3);

        // Simulate write completion sync
        app.sync_baseline();
        assert!(!app.is_dirty());
        assert_eq!(app.dirty_categories().len(), 0);
    }

    #[test]
    fn led_dirty_includes_block38() {
        let mut app = PanteonApp::default();
        app.ledger.append(crate::model::ShadowEntry::new(0, 0x0018, 0x071b, "1000"));
        app.ledger.append(crate::model::ShadowEntry::new(7, 0x0038, 0x0740, "1000")); // baseline: 4
        app.sync_baseline();
        assert_eq!(app.led_brightness, 4);
        assert!(!app.is_dirty());

        let target_level = 6;
        app.led_brightness = target_level;
        assert!(app.is_dirty());
        assert!(app.dirty_categories().contains(&DirtyCategory::Led));

        app.request_cumulative_apply();
        assert_eq!(app.pending_blocks.len(), 2);
        assert_eq!(app.pending_blocks[0].addr, 0x0018);
        assert_eq!(app.pending_blocks[1].addr, 0x0038);

        let expected_bank7 = 0x0700 | (((8 - target_level) as u16) << 4);
        assert_eq!(
            app.pending_blocks[1].banks[7], expected_bank7,
            "banks[7] must equal 0x0700 | ((8-level)<<4)"
        );
    }

    #[test]
    fn brightness_change_detected_vs_baseline() {
        let mut app = PanteonApp::default();
        // Ledger has level 2 (0x0700 | ((8 - 2) << 4) = 0x0760)
        app.ledger.append(crate::model::ShadowEntry::new(7, 0x0038, 0x0760, "1000"));
        app.sync_baseline();

        // Baseline must be synchronized from ledger
        assert_eq!(app.led_brightness, 2);
        assert_eq!(app.baseline.led_brightness, 2);
        assert!(!app.is_dirty());

        // Modifying brightness triggers dirty state
        app.led_brightness = 8;
        assert!(app.is_dirty());
        let dirty = app.dirty_categories();
        assert!(dirty.contains(&DirtyCategory::Led));

        let diffs = app.diff_entries();
        assert!(diffs.iter().any(|d| d.0 == "Яркость LED"));

        // Reverting brightness restores clean state
        app.led_brightness = 2;
        assert!(!app.is_dirty());
    }

    #[test]
    fn test_led_plan_frame_count_is_sum_of_sessions() {
        let mut app = PanteonApp::default();
        app.ledger.append(crate::model::ShadowEntry::new(0, 0x0018, 0x071b, "1000"));
        app.ledger.append(crate::model::ShadowEntry::new(7, 0x0038, 0x0740, "1000"));
        app.sync_baseline();

        let target_level = 7;
        app.led_brightness = target_level;
        app.plan_led_with_summary();

        let summary = app.dry_run_summary.as_ref().expect("LED dry-run summary must exist");

        // Calculate expected frame counts independently
        let opt = &crate::gui::LED_EFFECT_OPTIONS[app.selected_effect_idx];
        let wire_byte = opt.spec.wire_byte(app.led_speed);
        let mask = crate::codec::led::encode_palette_mask(app.all_colors, &app.selected_colors);
        let mut overrides_18 = vec![
            (crate::model::registers::LED_EFFECT_SPEED.0, 0x0700 | wire_byte as u16),
            (crate::model::registers::LED_COLOR_PALETTE.0, 0x0700 | mask as u16),
        ];
        if let Some(slot) = app.active_slot_override {
            if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(slot) {
                overrides_18.push((crate::model::registers::ACTIVE_DPI_SLOT.0, raw_slot));
            }
        } else if !app.ledger.entries.iter().any(|e| e.bank == 0 && e.addr == 0x0018) {
            if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(app.selected_dpi_slot) {
                overrides_18.push((crate::model::registers::ACTIVE_DPI_SLOT.0, raw_slot));
            }
        }
        overrides_18.push((crate::model::registers::FIRE_DELAY_LOW.0, 0x0700 | (app.fire_delay & 0xff)));
        overrides_18.push((crate::model::registers::FIRE_DELAY_HIGH.0, 0x0700 | ((app.fire_delay >> 8) & 0xff)));

        let b18 = crate::service::resolve_block18(&app.ledger, &overrides_18).expect("resolve block 18");
        let b38 = crate::service::resolve_block38(&app.ledger, Some(app.led_brightness)).expect("resolve block 38");

        let session18_frames = crate::service::plan_block_session_frames(&[b18], None, false).len();
        let session38_frames = crate::service::plan_block_session_frames(&[b38], None, false).len();

        assert_eq!(summary.frame_count, session18_frames + session38_frames);
        assert_eq!(summary.all_frames.len(), session18_frames + session38_frames);
        assert!(summary.description.contains(&format!("Сессия 0x0018: {} кадров", session18_frames)));
        assert!(summary.description.contains(&format!("Сессия 0x0038: {} кадров", session38_frames)));
        let expected_bank7 = 0x0700 | (((8 - target_level) as u16) << 4);
        assert!(summary.description.contains(&format!("0x{:04x}", expected_bank7)));
    }

    #[test]
    fn test_primary_cta_polling_flow_opens_modal() {
        let mut app = PanteonApp::default();
        assert!(!app.is_dirty());
        assert!(!app.confirm_modal_open);

        // User changes polling rate to 250 Hz
        app.selected_polling = 250;
        let dirty = app.dirty_categories();
        assert!(dirty.contains(&DirtyCategory::Polling));
        assert!(app.is_dirty());

        // Clicking primary CTA on DPI tab invokes request_polling_write()
        app.request_polling_write();
        assert!(app.confirm_modal_open);
        assert!(app.pending_plan.is_some());
        let plan = app.pending_plan.as_ref().unwrap();
        assert_eq!(plan.bank, 6);
        assert_eq!(plan.data, 0x0703); // Confirmed 250 Hz code

        // Cancel confirm resets pending
        app.cancel_confirm();
        assert!(!app.confirm_modal_open);
        assert!(app.pending_plan.is_none());
    }
}
