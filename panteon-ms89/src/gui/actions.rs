use std::path::PathBuf;

use crate::codec::led::encode_palette_mask;
use crate::device::WriteGateLevel;
use crate::gui::app::PanteonApp;
use crate::gui::LED_EFFECT_OPTIONS;
use crate::model::registers;
use crate::service::{
    execute_block_session, execute_with_commit, is_multi_bank_block,
    load_snapshot_from_file, open_device, plan_color_session, plan_dpi_slot, plan_dpi_slots,
    plan_fire_delay, plan_led_effect_speed, plan_polling_rate, plans_to_block_writes, record_write,
    resolve_block18, resolve_state_path, restore_snapshot,
};

impl PanteonApp {
    pub fn plan_polling(&mut self) {
        match plan_polling_rate(self.selected_polling) {
            Ok(plan) => {
                let mut overrides = vec![(plan.bank, plan.data)];
                if let Some(slot) = self.active_slot_override {
                    if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(slot) {
                        overrides.push((registers::ACTIVE_DPI_SLOT.0, raw_slot));
                    }
                }
                match resolve_block18(&self.ledger, &overrides) {
                    Ok(block) => {
                        self.status_message = format!(
                            "[Dry-run Plan] Polling rate {} Hz (Bank {}, Addr 0x{:04x}, Data 0x{:04x}, Bank 0: 0x{:04x})",
                            self.selected_polling, plan.bank, plan.addr, plan.data, block.banks[0]
                        );
                    }
                    Err(e) => self.status_message = format!("Error planning polling: {}", e),
                }
            }
            Err(e) => self.status_message = format!("Error planning polling: {}", e),
        }
    }

    pub fn request_polling_write(&mut self) {
        if let Ok(plan) = plan_polling_rate(self.selected_polling) {
            self.clear_pending();
            self.pending_plan = Some(plan);
            self.confirm_modal_open = true;
        }
    }

    pub fn plan_dpi_slot(&mut self) {
        match plan_dpi_slot(self.selected_dpi_slot) {
            Ok(plan) => {
                match resolve_block18(&self.ledger, &[(plan.bank, plan.data)]) {
                    Ok(block) => {
                        self.status_message = format!(
                            "[Dry-run Plan] Active DPI Slot {} (Bank {}, Addr 0x{:04x}, Data 0x{:04x}, Bank 0: 0x{:04x})",
                            self.selected_dpi_slot, plan.bank, plan.addr, plan.data, block.banks[0]
                        );
                    }
                    Err(e) => self.status_message = format!("Error planning DPI: {}", e),
                }
            }
            Err(e) => self.status_message = format!("Error planning DPI: {}", e),
        }
    }

    pub fn request_dpi_write(&mut self) {
        if let Ok(plan) = plan_dpi_slot(self.selected_dpi_slot) {
            self.clear_pending();
            self.pending_plan = Some(plan);
            self.confirm_modal_open = true;
        }
    }

    fn current_rgb_colors(&self) -> [(u8, u8, u8); 6] {
        let mut res = [(0, 0, 0); 6];
        for (i, c) in self.slot_colors.iter().enumerate() {
            res[i] = (c[0], c[1], c[2]);
        }
        res
    }

    pub fn plan_dpi_full_configuration(&mut self) {
        match plan_dpi_slots(&self.dpi_slots) {
            Ok(dpi_plans) => {
                let colors = self.current_rgb_colors();
                match plan_color_session(&colors, &crate::codec::DEFAULT_SECOND_COLORS, &self.ledger, Some(self.led_brightness)) {
                    Ok(color_plans) => {
                        let b7 = color_plans.iter().find(|p| p.addr == 0x0038 && p.bank == 7).map(|p| p.data).unwrap_or(0x0700);
                        self.status_message = format!(
                            "[Dry-run Plan] Full DPI: {:?}, Active: Slot {}, Brightness: {} (Bank 7: 0x{:04x}), {} DPI plans, {} color plans. Full 867-frame session with flash commit planned.",
                            self.dpi_slots, self.selected_dpi_slot, self.led_brightness, b7, dpi_plans.len(), color_plans.len()
                        );
                    }
                    Err(e) => self.status_message = format!("Error planning colors: {}", e),
                }
            }
            Err(e) => self.status_message = format!("Error planning full DPI: {}", e),
        }
    }

    pub fn request_dpi_full_write(&mut self) {
        if let Ok(dpi_plans) = plan_dpi_slots(&self.dpi_slots) {
            let colors = self.current_rgb_colors();
            match plan_color_session(&colors, &crate::codec::DEFAULT_SECOND_COLORS, &self.ledger, Some(self.led_brightness)) {
                Ok(color_plans) => {
                    // Record DPI slots into ledger
                    for p in &dpi_plans {
                        self.ledger.append(crate::model::ShadowEntry::new(p.bank, p.addr, p.data, crate::model::current_timestamp_secs()));
                    }

                    // Record colors into ledger
                    for p in &color_plans {
                        self.ledger.append(crate::model::ShadowEntry::new(p.bank, p.addr, p.data, crate::model::current_timestamp_secs()));
                    }

                    // Record active DPI slot into ledger
                    let slot_val = 0x0718 | (self.selected_dpi_slot as u16 - 1);
                    self.ledger.append(crate::model::ShadowEntry::new(0, 0x0018, slot_val, crate::model::current_timestamp_secs()));

                    // Build Full-Push snapshot (867 frames with correct delays and tail)
                    match self.build_editor_full_push_snapshot() {
                        Ok(mut snap) => {
                            snap.label = "apply-dpi-colors".to_string();
                            self.clear_pending();
                            self.pending_snapshot = Some(snap);
                            self.confirm_modal_open = true;
                        }
                        Err(e) => {
                            self.status_message = format!("Failed to build Full-Push snapshot: {}", e);
                        }
                    }
                }
                Err(e) => {
                    self.status_message = format!("Error planning colors: {}", e);
                }
            }
        }
    }

    pub fn plan_fire_delay_action(&mut self) {
        match plan_fire_delay(self.fire_delay) {
            Ok((p_low, p_high)) => {
                let mut overrides = vec![
                    (registers::FIRE_DELAY_LOW.0, p_low.data),
                    (registers::FIRE_DELAY_HIGH.0, p_high.data),
                ];
                if let Some(slot) = self.active_slot_override {
                    if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(slot) {
                        overrides.push((registers::ACTIVE_DPI_SLOT.0, raw_slot));
                    }
                }
                match resolve_block18(&self.ledger, &overrides) {
                    Ok(block) => {
                        self.status_message = format!(
                            "[Dry-run Plan] Fire Delay {} ms (Bank 2: 0x{:02x}, Bank 3: 0x{:02x}, Bank 0: 0x{:04x})",
                            self.fire_delay, p_low.data, p_high.data, block.banks[0]
                        );
                    }
                    Err(e) => self.status_message = format!("Error planning fire delay: {}", e),
                }
            }
            Err(e) => self.status_message = format!("Error planning fire delay: {}", e),
        }
    }

    pub fn request_fire_delay_write(&mut self) {
        self.clear_pending();
        self.pending_fire_delay = Some(self.fire_delay);
        self.confirm_modal_open = true;
    }

    pub fn plan_led(&mut self) {
        let opt = &LED_EFFECT_OPTIONS[self.selected_effect_idx];
        let wire_byte = opt.spec.wire_byte(self.led_speed);
        match plan_led_effect_speed(wire_byte >> 4, wire_byte & 0x0f) {
            Ok(plan) => {
                let mask = encode_palette_mask(self.all_colors, &self.selected_colors);
                let mut overrides = vec![
                    (plan.bank, plan.data),
                    (registers::LED_COLOR_PALETTE.0, 0x0700 | mask as u16),
                ];
                if let Some(slot) = self.active_slot_override {
                    if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(slot) {
                        overrides.push((registers::ACTIVE_DPI_SLOT.0, raw_slot));
                    }
                } else if !self.ledger.entries.iter().any(|e| e.bank == 0 && e.addr == 0x0018) {
                    if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(self.selected_dpi_slot) {
                        overrides.push((registers::ACTIVE_DPI_SLOT.0, raw_slot));
                    }
                }

                match resolve_block18(&self.ledger, &overrides) {
                    Ok(block) => {
                        let b38_res = crate::service::resolve_block38(&self.ledger, Some(self.led_brightness));
                        let b7_str = match b38_res {
                            Ok(ref b) => {
                                self.pending_blocks = vec![block.clone(), b.clone()];
                                self.pending_plan = None;
                                format!("0x{:04x}", b.banks[7])
                            }
                            Err(_) => "error".to_string(),
                        };
                        self.status_message = format!(
                            "[Dry-run Plan] LED Mode '{}', Speed {}, Brightness {} (Bank 7: {}), Mask 0x{:02x} (Byte 0x{:02x}, Data 0x{:04x}, Bank 0: 0x{:04x})",
                            opt.label, self.led_speed, self.led_brightness, b7_str, mask, wire_byte, plan.data, block.banks[0]
                        );
                    }
                    Err(e) => self.status_message = format!("Error planning LED: {}", e),
                }
            }
            Err(e) => self.status_message = format!("Error planning LED: {}", e),
        }
    }

    pub fn request_led_write(&mut self) {
        let opt = &LED_EFFECT_OPTIONS[self.selected_effect_idx];
        let wire_byte = opt.spec.wire_byte(self.led_speed);
        let mask = encode_palette_mask(self.all_colors, &self.selected_colors);
        let mut overrides = vec![
            (registers::LED_EFFECT_SPEED.0, 0x0700 | wire_byte as u16),
            (registers::LED_COLOR_PALETTE.0, 0x0700 | mask as u16),
        ];
        if let Some(slot) = self.active_slot_override {
            if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(slot) {
                overrides.push((registers::ACTIVE_DPI_SLOT.0, raw_slot));
            }
        } else if !self.ledger.entries.iter().any(|e| e.bank == 0 && e.addr == 0x0018) {
            if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(self.selected_dpi_slot) {
                overrides.push((registers::ACTIVE_DPI_SLOT.0, raw_slot));
            }
        }

        if let (Ok(b18), Ok(b38)) = (
            resolve_block18(&self.ledger, &overrides),
            crate::service::resolve_block38(&self.ledger, Some(self.led_brightness)),
        ) {
            self.clear_pending();
            self.pending_blocks = vec![b18, b38];
            self.confirm_modal_open = true;
        }
    }

    pub fn load_snapshot(&mut self) {
        self.snapshot_error = None;
        let p = PathBuf::from(&self.snapshot_path);
        match crate::gui::snapshot_panel::resolve_snapshot_path(&p) {
            Ok(resolved) => match load_snapshot_from_file(&resolved) {
                Ok(snap) => {
                    self.status_message = format!("Loaded snapshot '{}' ({} frames)", snap.label, snap.frames.len());
                    self.snapshot_preview = Some(snap);
                    self.snapshot_error = None;
                }
                Err(e) => {
                    let msg = format!("Failed to load snapshot: {}", e);
                    self.status_message = msg.clone();
                    self.snapshot_error = Some(msg);
                }
            },
            Err(e) => {
                self.status_message = e.clone();
                self.snapshot_error = Some(e);
            }
        }
    }

    pub fn plan_restore(&mut self) {
        if let Some(snap) = &self.snapshot_preview {
            self.snapshot_error = None;
            self.status_message = format!("[Dry-run Plan] Replay {} frames for snapshot '{}'", snap.frames.len(), snap.label);
        } else {
            self.snapshot_error = Some("Снапшот не загружен. Нажмите '📥 Load' перед планированием.".to_string());
        }
    }

    pub fn clear_pending(&mut self) {
        self.pending_plan = None;
        self.pending_plans.clear();
        self.pending_blocks.clear();
        self.pending_dirty_categories.clear();
        self.pending_fire_delay = None;
        self.pending_snapshot = None;
    }

    pub fn request_factory_reset(&mut self) {
        self.snapshot_error = None;
        match crate::service::load_factory_reset_snapshot() {
            Ok(snap) => {
                self.clear_pending();
                self.pending_snapshot = Some(snap);
                self.confirm_modal_open = true;
            }
            Err(e) => {
                let msg = format!("Failed to load embedded factory reset snapshot: {}", e);
                self.status_message = msg.clone();
                self.snapshot_error = Some(msg);
            }
        }
    }

    pub fn request_full_push(&mut self) {
        self.snapshot_error = None;
        match self.build_editor_full_push_snapshot() {
            Ok(snap) => {
                self.clear_pending();
                self.pending_snapshot = Some(snap);
                self.confirm_modal_open = true;
            }
            Err(e) => {
                let msg = format!("Failed to build full-push snapshot: {}", e);
                self.status_message = msg.clone();
                self.snapshot_error = Some(msg);
            }
        }
    }

    pub fn request_restore_write(&mut self) {
        self.snapshot_error = None;
        if let Some(snap) = self.snapshot_preview.clone() {
            self.clear_pending();
            self.pending_snapshot = Some(snap);
            self.confirm_modal_open = true;
        } else {
            self.snapshot_error = Some("Снапшот не загружен. Нажмите '📥 Load' перед записью.".to_string());
        }
    }

    pub fn cancel_confirm(&mut self) {
        self.confirm_modal_open = false;
        self.clear_pending();
    }

    pub fn execute_confirmed_write(&mut self) {
        let dev_path = match self.current_device() {
            Some(d) => d.path.clone(),
            None => {
                self.status_message = "Write error: No device selected".to_string();
                self.cancel_confirm();
                return;
            }
        };

        if let Some(plan) = self.pending_plan.take() {
            match open_device(Some(dev_path.clone()), WriteGateLevel::Level2ConfigurationWrite) {
                Ok((target_path, mut dev)) => {
                    let res = if is_multi_bank_block(plan.addr) {
                        let block_res = if plan.addr == 0x0018 {
                            let mut overrides = vec![(plan.bank, plan.data)];
                            if let Some(slot) = self.active_slot_override {
                                if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(slot) {
                                    overrides.push((registers::ACTIVE_DPI_SLOT.0, raw_slot));
                                }
                            }
                            resolve_block18(&self.ledger, &overrides).map(|b18| {
                                if plan.bank == registers::LED_EFFECT_SPEED.0 {
                                    if let Ok(b38) = crate::service::resolve_block38(&self.ledger, Some(self.led_brightness)) {
                                        vec![b18, b38]
                                    } else {
                                        vec![b18]
                                    }
                                } else {
                                    vec![b18]
                                }
                            })
                        } else {
                            Ok(plans_to_block_writes(&[plan.clone()]))
                        };
                        match block_res {
                            Ok(blocks) => execute_block_session(&mut dev, &blocks, false, true, 0),
                            Err(e) => Err(e),
                        }
                    } else {
                        execute_with_commit(&mut dev, &plan, false, true)
                    };
                    match res {
                        Ok(()) => {
                            let state_path = resolve_state_path(None);
                            let _ = record_write(&state_path, &target_path, plan.bank, plan.addr, plan.data);
                            if plan.addr == 0x0018 && plan.bank == registers::LED_EFFECT_SPEED.0 {
                                if let Ok(b38) = crate::service::resolve_block38(&self.ledger, Some(self.led_brightness)) {
                                    let _ = record_write(&state_path, &target_path, 7, 0x0038, b38.banks[7]);
                                }
                            }
                            if let Some(slot) = self.active_slot_override {
                                if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(slot) {
                                    let _ = record_write(&state_path, &target_path, registers::ACTIVE_DPI_SLOT.0, registers::ACTIVE_DPI_SLOT.1, raw_slot);
                                }
                            }
                            self.reload_ledger();
                            self.status_message = format!("Write transaction executed successfully! Target: Bank {}, Addr 0x{:04x} (commits to flash)", plan.bank, plan.addr);
                            self.execute_soft_reset();
                        }
                        Err(e) => self.status_message = format!("Write execution error: {}", e),
                    }
                }
                Err(e) => self.status_message = format!("Failed to open device for write: {}", e),
            }
        } else if let Some(delay_ms) = self.pending_fire_delay.take() {
            match open_device(Some(dev_path.clone()), WriteGateLevel::Level2ConfigurationWrite) {
                Ok((target_path, mut dev)) => {
                    let mut overrides = vec![
                        (registers::FIRE_DELAY_LOW.0, 0x0700 | (delay_ms & 0xff)),
                        (registers::FIRE_DELAY_HIGH.0, 0x0700 | ((delay_ms >> 8) & 0xff)),
                    ];
                    if let Some(slot) = self.active_slot_override {
                        if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(slot) {
                            overrides.push((registers::ACTIVE_DPI_SLOT.0, raw_slot));
                        }
                    }
                    let res = resolve_block18(&self.ledger, &overrides).and_then(|block| {
                        execute_block_session(&mut dev, &[block], false, true, 0)
                    });
                    match res {
                        Ok(()) => {
                            let state_path = resolve_state_path(None);
                            let _ = record_write(&state_path, &target_path, registers::FIRE_DELAY_LOW.0, registers::FIRE_DELAY_LOW.1, 0x0700 | (delay_ms & 0xff));
                            let _ = record_write(&state_path, &target_path, registers::FIRE_DELAY_HIGH.0, registers::FIRE_DELAY_HIGH.1, 0x0700 | ((delay_ms >> 8) & 0xff));
                            if let Some(slot) = self.active_slot_override {
                                if let Ok(raw_slot) = crate::model::DeviceState::slot_index_to_raw(slot) {
                                    let _ = record_write(&state_path, &target_path, registers::ACTIVE_DPI_SLOT.0, registers::ACTIVE_DPI_SLOT.1, raw_slot);
                                }
                            }
                            self.reload_ledger();
                            self.status_message = format!("Fire delay ({} ms) written successfully! (commits to flash)", delay_ms);
                            self.execute_soft_reset();
                        }
                        Err(e) => self.status_message = format!("Fire delay write error: {}", e),
                    }
                }
                Err(e) => self.status_message = format!("Failed to open device for write: {}", e),
            }
        } else if let Some(snap) = self.pending_snapshot.take() {
            match open_device(Some(dev_path.clone()), WriteGateLevel::Level2ConfigurationWrite) {
                Ok((_target_path, mut dev)) => {
                    let mut ledger = Some(&mut self.ledger);
                    match restore_snapshot(&mut dev, &snap, None, false, &mut ledger) {
                        Ok(()) => {
                            let state_path = resolve_state_path(None);
                            let _ = crate::service::save_ledger_atomic(&state_path, &self.ledger);
                            self.reload_ledger();
                            self.sync_baseline();
                            self.status_message = format!("Full state '{}' applied successfully! Soft-reset triggered.", snap.label);
                            self.execute_soft_reset();
                        }
                        Err(e) => {
                            let msg = format!("Apply error: {}", e);
                            self.status_message = msg.clone();
                            self.snapshot_error = Some(msg);
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("Failed to open device for apply: {}", e);
                    self.status_message = msg.clone();
                    self.snapshot_error = Some(msg);
                }
            }
        } else if !self.pending_blocks.is_empty() {
            let blocks = std::mem::take(&mut self.pending_blocks);
            match open_device(Some(dev_path.clone()), WriteGateLevel::Level2ConfigurationWrite) {
                Ok((target_path, mut dev)) => {
                    match execute_block_session(&mut dev, &blocks, false, true, 0) {
                        Ok(()) => {
                            let state_path = resolve_state_path(None);
                            for b in &blocks {
                                for (bank, &data) in b.banks.iter().enumerate() {
                                    let _ = record_write(&state_path, &target_path, bank as u8, b.addr, data);
                                }
                            }
                            self.reload_ledger();
                            self.sync_baseline();
                            self.status_message = format!("Cumulative write executed successfully! ({} blocks)", blocks.len());
                            self.set_status(crate::gui::StatusKind::Success, "Изменения успешно записаны во Flash-память мыши");
                            self.execute_soft_reset();
                        }
                        Err(e) => {
                            let msg = format!("Cumulative write execution error: {}", e);
                            self.status_message = msg.clone();
                            self.set_status(crate::gui::StatusKind::Error, msg);
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("Failed to open device for cumulative write: {}", e);
                    self.status_message = msg.clone();
                    self.set_status(crate::gui::StatusKind::Error, msg);
                }
            }
        }

        self.confirm_modal_open = false;
    }

    pub fn execute_soft_reset(&mut self) {
        let bus_dev = self.current_device().and_then(|d| match (d.usb_bus, d.usb_dev) {
            (Some(b), Some(dev)) => Some((b, dev)),
            _ => None,
        });

        let (bus, dev) = match bus_dev {
            Some(pair) => pair,
            None => {
                self.status_message = "Soft Reset error: USB bus/dev unknown for selected device".to_string();
                return;
            }
        };

        match crate::service::soft_reset((bus, dev)) {
            Ok(()) => {
                self.status_message = format!("USB Soft Reset triggered for /dev/bus/usb/{:03}/{:03}. Waiting for re-enumeration...", bus, dev);
                std::thread::sleep(std::time::Duration::from_millis(300));
                self.refresh_devices();
                self.query_device_state();
                self.status_message = format!("USB Soft Reset complete. Devices re-discovered ({})", self.devices.len());
            }
            Err(e) => {
                self.status_message = format!("USB Soft Reset error: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ShadowEntry;

    #[test]
    fn test_gui_dry_run_shows_bank0_from_ledger() {
        let mut app = PanteonApp::default();
        app.ledger.append(ShadowEntry::new(0, 0x0018, 0x071b, "1000")); // Slot 4

        app.plan_polling();
        assert!(app.status_message.contains("Bank 0: 0x071b"));

        app.plan_led();
        assert!(app.status_message.contains("Bank 0: 0x071b"));

        app.plan_fire_delay_action();
        assert!(app.status_message.contains("Bank 0: 0x071b"));
    }

    #[test]
    fn test_gui_dry_run_override_active_slot() {
        let mut app = PanteonApp::default();
        app.ledger.append(ShadowEntry::new(0, 0x0018, 0x0718, "1000")); // Slot 1 in ledger
        app.active_slot_override = Some(4); // Override to slot 4

        app.plan_polling();
        assert!(app.status_message.contains("Bank 0: 0x071b"));

        app.plan_led();
        assert!(app.status_message.contains("Bank 0: 0x071b"));
    }
}
