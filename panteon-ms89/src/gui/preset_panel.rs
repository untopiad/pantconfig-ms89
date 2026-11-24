use std::path::{Path, PathBuf};

use eframe::egui;

use crate::gui::app::{PanteonApp, StatusKind};
use crate::gui::theme;
use crate::model::Profile;
use crate::store::presets;
use crate::PanteonError;

/// Directory where named profile presets (*.toml) are stored next to the ledger.
pub fn presets_dir() -> PathBuf {
    crate::service::resolve_state_path(None)
        .parent()
        .map(|p| p.join("presets"))
        .unwrap_or_else(|| PathBuf::from("presets"))
}

/// Lists saved preset names (file stems of *.toml in the presets dir), sorted.
/// Thin wrapper over [`crate::store::presets::list`] — the GUI does no file I/O.
pub fn list_presets() -> Vec<String> {
    presets::list(&presets_dir())
}

/// Minimal `which`: resolves `name` to an executable file on `PATH`.
#[cfg(target_os = "linux")]
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// First file-manager binary available on `PATH`, preferring the current desktop's.
#[cfg(target_os = "linux")]
fn file_manager_program() -> Option<&'static str> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();

    const KDE: &[&str] = &["dolphin", "nautilus", "nemo", "thunar", "caja", "pcmanfm"];
    const XFCE: &[&str] = &["thunar", "nautilus", "dolphin", "nemo", "caja", "pcmanfm"];
    const CINNAMON: &[&str] = &["nemo", "nautilus", "dolphin", "thunar", "caja", "pcmanfm"];
    const GENERIC: &[&str] = &["nautilus", "dolphin", "nemo", "thunar", "caja", "pcmanfm"];

    let order = if desktop.contains("kde") {
        KDE
    } else if desktop.contains("xfce") {
        XFCE
    } else if desktop.contains("cinnamon") {
        CINNAMON
    } else {
        GENERIC
    };

    order.iter().copied().find(|name| which(name).is_some())
}

/// Opens `path` in the desktop file manager.
///
/// A file-manager binary is launched directly; `xdg-open` is only the fallback.
/// Rationale: `xdg-open` delegates to the desktop's MIME helpers, and when that
/// chain is unhealthy it routes a *directory* to an unrelated handler — observed
/// on the reporter's machine, where «Открыть папку профилей» popped a KDE Connect
/// dialog («Device to open <path> on:») and the folder opened only after closing
/// it. Launching `nautilus`/`dolphin`/etc. skips that chain entirely.
fn open_in_file_manager(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if let Some(program) = file_manager_program() {
            match std::process::Command::new(program).arg(path).spawn() {
                Ok(_) => return Ok(()),
                Err(_) => { /* fall through to xdg-open */ }
            }
        }
        return std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("не удалось запустить файловый менеджер: {}", e));
    }

    #[cfg(target_os = "macos")]
    {
        return std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("не удалось запустить Finder: {}", e));
    }

    #[cfg(target_os = "windows")]
    {
        return std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("не удалось запустить проводник: {}", e));
    }

    #[allow(unreachable_code)]
    Ok(())
}

/// Width of the profile selector (Apple-style compact popup button).
fn selector_width() -> f32 {
    260.0
}

/// Placeholder shown in the selector when nothing is selected.
fn selector_placeholder(presets: &[String]) -> &'static str {
    if presets.is_empty() {
        "Нет профилей"
    } else {
        "Выберите профиль…"
    }
}

/// Drops a selection whose profile no longer exists (e.g. the file was deleted
/// outside the app). Pure so the stale-guard is unit-testable; returns `true`
/// when the selection was cleared. Never panics, never touches the filesystem.
fn resolve_stale_selection(selected: &mut Option<String>, available: &[String]) -> bool {
    match selected {
        Some(name) if !available.iter().any(|n| n == name) => {
            *selected = None;
            true
        }
        _ => false,
    }
}

/// Compares only the parts of a [`Profile`] that the editor actually represents.
///
/// `Profile` also carries fields the GUI never edits: `id`, `name`,
/// `button_bindings` and the second colour channel `DpiSlot::second` (the editor
/// edits `main`; saving always writes `DEFAULT_SECOND_COLORS` — the known
/// model↔codec trap, AGENTS §2). A whole-struct `==` would therefore report a
/// false «modified» on any loaded profile with a custom name, bindings or second
/// channel, so the R18 indicator compares the fields below only.
pub fn profile_config_eq(a: &Profile, b: &Profile) -> bool {
    // A truncated level list cannot represent the editor's 6 slots.
    if a.dpi_levels.len() != 6 || b.dpi_levels.len() != 6 {
        return false;
    }

    a.active_dpi_index == b.active_dpi_index
        && a.polling_rate == b.polling_rate
        && a.fire_delay_ms == b.fire_delay_ms
        && a.led_effect == b.led_effect
        && a.led_speed == b.led_speed
        && a.led_brightness == b.led_brightness
        && a.led_all_colors == b.led_all_colors
        && a.dpi_levels.iter().zip(&b.dpi_levels).all(|(x, y)| {
            x.level_index == y.level_index
                && x.dpi == y.dpi
                && x.enabled == y.enabled
                && x.main == y.main
        })
}

/// Render the Profile Presets panel (R17: one selector + shared actions).
///
/// Surface: `[ profile ▾ ] [Сохранить] [✕]` then `[+ Новый профиль] [Импорт] [Экспорт]`.
/// The per-profile rows (`▸ name` / `−`), the free-standing name field and the
/// contextual «Обновить / Сохранить текущий» button of R16 are gone: the name
/// lives only in the selector, and the two actions always apply to the selection.
pub fn render_preset_panel(ui: &mut egui::Ui, app: &mut PanteonApp) {
    let dir = presets_dir();
    let presets = presets::list(&dir);

    // Stale-selection guard, every frame: the presets directory can change under
    // us (file deleted/created outside the app) — never keep a dead selection.
    if resolve_stale_selection(&mut app.selected_preset, &presets) {
        // The saved state of a vanished profile is meaningless as well.
        app.selected_preset_snapshot = None;
    }

    if presets.is_empty() {
        ui.label(
            egui::RichText::new(
                "Профилей пока нет — создайте первый через «+ Новый профиль».",
            )
            .small()
            .weak(),
        );
    }

    ui.horizontal_wrapped(|ui| {
        // One control height for the whole toolbar row (selector + both actions).
        ui.spacing_mut().interact_size.y = theme::PILL_H;
        render_selector(ui, app, &presets);
        render_toolbar(ui, app);
    });

    ui.add_space(6.0);

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().interact_size.y = theme::PILL_H;

        // Creating a profile is a first-class panel action, not an item hidden in
        // the dropdown: it stays visible (and enabled) even when no profile exists.
        let new_profile = ui.add(
            egui::Button::new("+ Новый профиль").min_size(egui::vec2(0.0, theme::PILL_H)),
        );
        if theme::tooltip(new_profile, "Создать профиль из текущей конфигурации").clicked() {
            app.preset_name_input.clear();
            app.preset_new_modal_open = true;
        }

        if theme::tooltip(ui.button("Импорт"), "Импорт профиля из файла TOML").clicked() {
            import_profile_dialog(app);
        }
        if theme::tooltip(ui.button("Экспорт"), "Экспорт текущей конфигурации в файл TOML")
            .clicked()
        {
            export_profile_dialog(app);
        }
    });

    ui.add_space(6.0);

    // Settings folder as a clickable link (opens in file manager)
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Папка профилей:").small().weak());
        if theme::tooltip(
            ui.button(dir.display().to_string()),
            "Открыть папку профилей в проводнике",
        )
        .clicked()
        {
            if let Err(e) = open_in_file_manager(&dir) {
                app.set_status(StatusKind::Error, format!("Не удалось открыть папку: {}", e));
            }
        }
    });
}

/// Dropdown of saved profiles: the active one carries a «✓ » prefix, the last
/// item used to live in the menu; since R18 it is a permanent panel button.
fn render_selector(ui: &mut egui::Ui, app: &mut PanteonApp, presets: &[String]) {
    let selected = app.selected_preset.clone();
    let selected_text = selected
        .clone()
        .unwrap_or_else(|| selector_placeholder(presets).to_string());

    // `Some(name)` = switch to that profile. The list contains saved profiles only:
    // «+ Новый профиль» lives on the panel (always visible), not inside the menu.
    let mut chosen: Option<String> = None;

    // Nothing to choose from -> a disabled selector («Нет профилей»), no empty popup.
    ui.add_enabled_ui(!presets.is_empty(), |ui| {
        egui::ComboBox::from_id_salt("preset_selector")
            .width(selector_width())
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for name in presets {
                    let is_active = selected.as_deref() == Some(name.as_str());
                    let label = if is_active {
                        format!("✓ {}", name)
                    } else {
                        name.clone()
                    };
                    if ui.selectable_label(is_active, label).clicked() && !is_active {
                        // Guard: clicking the already active profile is a no-op
                        // (no reload, no confirmation, no dirty changes).
                        chosen = Some(name.clone());
                    }
                }
            });
    });

    if let Some(name) = chosen {
        request_preset_switch(app, &name);
    }
}

/// Shared actions over the current selection: «Сохранить» (overwrite) and «✕»
/// (delete). Both are disabled without a selection.
fn render_toolbar(ui: &mut egui::Ui, app: &mut PanteonApp) {
    let selection = app.selected_preset.clone();
    let has_selection = selection.is_some();

    // R18: does the editor still match the *saved* profile? Recomputed every frame
    // from `build_profile_from_app` vs the snapshot taken at load/save time — pure,
    // no file I/O. Limitation (documented, acceptable): editing the preset file
    // externally while it stays selected is not detected; the stale-guard covers
    // deletion only, and per-frame file reads are forbidden by the R16 boundary.
    let modified = match (&selection, &app.selected_preset_snapshot) {
        (Some(_), Some(snapshot)) => !profile_config_eq(&build_profile_from_app(app), snapshot),
        _ => false,
    };

    let save_label = if modified {
        egui::RichText::new("Сохранить")
            .strong()
            .color(egui::Color32::WHITE)
    } else {
        egui::RichText::new("Сохранить")
    };
    let mut save_button =
        egui::Button::new(save_label).min_size(egui::vec2(110.0, theme::PILL_H));
    if modified {
        save_button = save_button.fill(theme::ACCENT);
    }

    let save = ui.add_enabled(has_selection, save_button);
    let save = match (&selection, modified) {
        (Some(name), true) => theme::tooltip(
            save,
            format!(
                "Конфигурация отличается от сохранённого профиля „{}“ — сохраните изменения",
                name
            ),
        ),
        (Some(name), false) => theme::tooltip(
            save,
            format!(
                "Перезаписать профиль „{}“ текущей конфигурацией (в файл, не в мышь)",
                name
            ),
        ),
        (None, _) => theme::disabled_tooltip(save, "Сначала выберите профиль"),
    };
    if save.clicked() {
        // Existing R16 confirmation modal, text and behaviour unchanged.
        app.preset_overwrite_confirm = selection.clone();
    }

    let delete_button = egui::Button::new(egui::RichText::new("✕").color(theme::DANGER))
        .min_size(egui::vec2(40.0, theme::PILL_H));
    let delete = ui.add_enabled(has_selection, delete_button);
    let delete = match &selection {
        Some(name) => theme::tooltip(delete, format!("Удалить профиль „{}“", name)),
        None => theme::disabled_tooltip(delete, "Сначала выберите профиль"),
    };
    if delete.clicked() {
        // Existing R16 confirmation modal, text and behaviour unchanged.
        app.preset_delete_confirm = selection.clone();
    }
}

/// Switches to `name`: loads it into the editor, unless the editor has unsaved
/// edits — then the switch is gated behind a confirmation modal (never drop
/// user edits silently).
fn request_preset_switch(app: &mut PanteonApp, name: &str) {
    if app.dirty_categories().is_empty() {
        let dir = presets_dir();
        load_preset_into_editor(app, &dir, name);
    } else {
        app.preset_switch_confirm = Some(name.to_string());
    }
}

/// Loads preset `name` into the editor and makes it the current selection.
fn load_preset_into_editor(app: &mut PanteonApp, dir: &Path, name: &str) {
    match presets::load(dir, name) {
        Ok(mut profile) => {
            // R20: wire brightness 0 means «100%» (nibble 8 is outside the OEM
            // domain 0..7). Normalize before the editor *and* before the snapshot,
            // otherwise R18 reports a false «profile modified».
            profile.led_brightness =
                crate::codec::normalize_brightness_level(profile.led_brightness);
            load_profile_into_app(app, &profile);
            app.selected_preset = Some(name.to_string());
            // Saved state the R18 indicator compares against (no re-read per frame).
            app.selected_preset_snapshot = Some(profile);
            app.set_status(
                StatusKind::Success,
                format!("Загружен профиль '{}'. Проверьте и примените.", name),
            );
        }
        Err(e) => app.set_status(
            StatusKind::Error,
            format!("Не удалось прочитать '{}': {}", name, presets::error_message(&e)),
        ),
    }
}

/// Overwrites an existing preset after confirmation. The name field is kept so the
/// user continues working with the same profile.
fn overwrite_preset(app: &mut PanteonApp, name: &str) {
    let dir = presets_dir();
    let profile = build_profile_from_app(app);
    match presets::save(&dir, name, &profile) {
        Ok(_) => {
            // Exactly what was written is now the saved state -> indicator goes quiet.
            app.selected_preset_snapshot = Some(profile);
            app.set_status(StatusKind::Success, format!("Профиль '{}' обновлён.", name));
        }
        Err(e) => app.set_status(
            StatusKind::Error,
            format!("Не удалось сохранить профиль: {}", presets::error_message(&e)),
        ),
    }
}

/// Deletes a preset after confirmation.
fn delete_preset(app: &mut PanteonApp, name: &str) {
    let dir = presets_dir();
    match presets::delete(&dir, name) {
        Ok(()) => {
            // Nothing is selected once the file is gone (stale-guard would also
            // catch it on the next frame, this is the explicit contract).
            app.selected_preset = None;
            app.selected_preset_snapshot = None;
            app.set_status(StatusKind::Success, format!("Профиль '{}' удалён.", name));
        }
        Err(e) => app.set_status(
            StatusKind::Error,
            format!("Не удалось удалить '{}': {}", name, presets::error_message(&e)),
        ),
    }
}

fn import_profile_dialog(app: &mut PanteonApp) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("TOML Profile", &["toml"])
        .pick_file()
    else {
        return;
    };
    match presets::import_from_path(&path) {
        Ok(profile) => {
            load_profile_into_app(app, &profile);
            // The imported profile is not stored in the presets directory, so it
            // cannot be the current selection (nothing to overwrite/delete there).
            app.selected_preset = None;
            app.selected_preset_snapshot = None;
            app.set_status(
                StatusKind::Success,
                format!("Импортирован профиль '{}' из {}", profile.name, path.display()),
            );
        }
        Err(e) => app.set_status(
            StatusKind::Error,
            format!("Не удалось импортировать профиль: {}", presets::error_message(&e)),
        ),
    }
}

fn export_profile_dialog(app: &mut PanteonApp) {
    // Prefill the file name with the selected profile, so «Экспорт» produces
    // `base1.toml` by default; the dialog still allows typing any other name.
    let default_name = match app.selected_preset.as_deref() {
        Some(selected) => format!("{}.toml", selected),
        None => "profile.toml".to_string(),
    };
    let Some(path) = rfd::FileDialog::new()
        .add_filter("TOML Profile", &["toml"])
        .set_file_name(&default_name)
        .save_file()
    else {
        return;
    };
    let profile = build_profile_from_app(app);
    match presets::export_to_path(&path, &profile) {
        Ok(()) => app.set_status(
            StatusKind::Success,
            format!("Профиль сохранён в {}", path.display()),
        ),
        Err(e) => app.set_status(
            StatusKind::Error,
            format!("Не удалось сохранить профиль: {}", presets::error_message(&e)),
        ),
    }
}

/// Renders both small confirmation modals for the presets tab.
/// Destructive/irreversible actions ask first (Apple «Responsibility»); each
/// modal closes on «Отмена» and on Escape, and never keeps the background busy.
pub fn render_preset_modals(ctx: &egui::Context, app: &mut PanteonApp) {
    render_overwrite_confirm(ctx, app);
    render_delete_confirm(ctx, app);
    render_new_profile_modal(ctx, app);
    render_switch_confirm(ctx, app);
}

/// Validation state of the «Новый профиль» dialog. Pure: the enable/disable
/// decision (and the tooltip reason) is unit-testable without egui.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NewProfileState {
    /// Name is valid and free: creation is allowed.
    Ready,
    /// Name is rejected by `presets::validate_name` (reason for the tooltip).
    Invalid(String),
    /// Name is already used by a saved profile: creation must never overwrite.
    NameTaken,
}

fn new_profile_state(validation: Result<(), PanteonError>, name_taken: bool) -> NewProfileState {
    match validation {
        Err(err) => NewProfileState::Invalid(presets::error_message(&err)),
        Ok(()) if name_taken => NewProfileState::NameTaken,
        Ok(()) => NewProfileState::Ready,
    }
}

const NAME_TAKEN_REASON: &str =
    "Имя занято — выберите другое или обновите существующий профиль";

/// Creates a NEW profile from the current editor state.
/// Returns `true` when the modal may close (file written). Never overwrites: the
/// name is re-checked against the directory right before writing, because the
/// live validation of the modal can be stale if the folder changed meanwhile.
fn create_new_preset(app: &mut PanteonApp, name: &str) -> bool {
    let dir = presets_dir();
    if presets::list(&dir).iter().any(|n| n == name) {
        app.set_status(
            StatusKind::Error,
            format!("Профиль '{}' уже существует — создание отменено.", name),
        );
        return false;
    }

    let profile = build_profile_from_app(app);
    match presets::save(&dir, name, &profile) {
        Ok(_) => {
            app.selected_preset = Some(name.to_string());
            app.selected_preset_snapshot = Some(profile);
            app.preset_name_input.clear();
            app.set_status(StatusKind::Success, format!("Профиль '{}' создан.", name));
            true
        }
        Err(e) => {
            app.set_status(
                StatusKind::Error,
                format!("Не удалось создать профиль: {}", presets::error_message(&e)),
            );
            false
        }
    }
}

/// «Новый профиль» dialog: name field with live validation, never overwrites.
fn render_new_profile_modal(ctx: &egui::Context, app: &mut PanteonApp) {
    if !app.preset_new_modal_open {
        return;
    }

    let dir = presets_dir();
    let state = {
        let trimmed = app.preset_name_input.trim().to_string();
        let taken = presets::list(&dir).iter().any(|n| n == &trimmed);
        new_profile_state(presets::validate_name(&app.preset_name_input), taken)
    };
    let mut close = false;

    egui::Window::new("Новый профиль")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new("Создать профиль из текущей конфигурации?")
                    .strong()
                    .size(15.0),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(
                    "Будет создан из текущей конфигурации, включая несохранённые изменения.",
                )
                .color(theme::TEXT_SECONDARY),
            );
            ui.add_space(8.0);
            ui.add_sized(
                [selector_width(), 24.0],
                egui::TextEdit::singleline(&mut app.preset_name_input)
                    .hint_text("Имя профиля…"),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Отмена").clicked() {
                    close = true;
                }

                // Disabled = neutral fill + secondary text (same pattern as the
                // primary CTA): a dimmed accent still reads as "clickable".
                let ready = state == NewProfileState::Ready;
                let create = egui::Button::new(
                    egui::RichText::new("Создать").strong().color(if ready {
                        egui::Color32::WHITE
                    } else {
                        theme::TEXT_SECONDARY
                    }),
                );
                let create = if ready {
                    create.fill(theme::ACCENT)
                } else {
                    create.fill(theme::HOVER_BG)
                };
                let resp = ui.add_enabled(ready, create);
                let resp = match &state {
                    NewProfileState::Ready => theme::tooltip(resp, "Создать профиль"),
                    NewProfileState::Invalid(reason) => {
                        theme::disabled_tooltip(resp, reason.clone())
                    }
                    NewProfileState::NameTaken => {
                        theme::disabled_tooltip(resp, NAME_TAKEN_REASON)
                    }
                };
                if resp.clicked() {
                    let name = app.preset_name_input.trim().to_string();
                    if create_new_preset(app, &name) {
                        close = true;
                    }
                }
            });
        });

    if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.preset_new_modal_open = false;
        app.preset_name_input.clear();
    }
}

/// Confirmation shown when a profile switch would discard unsaved editor edits.
fn render_switch_confirm(ctx: &egui::Context, app: &mut PanteonApp) {
    let Some(name) = app.preset_switch_confirm.clone() else {
        return;
    };
    let mut close = false;

    egui::Window::new("Загрузить профиль")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!("Загрузить профиль „{}“?", name))
                    .strong()
                    .size(15.0),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Несохранённые изменения в редакторе будут потеряны.")
                    .color(theme::TEXT_SECONDARY),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Отмена").clicked() {
                    close = true;
                }
                // ACCENT, not DANGER: nothing on disk is destroyed, only the
                // unsaved editor state is replaced by the stored profile.
                let confirm = egui::Button::new(
                    egui::RichText::new("Загрузить")
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(theme::ACCENT);
                if ui.add(confirm).clicked() {
                    let dir = presets_dir();
                    load_preset_into_editor(app, &dir, &name);
                    close = true;
                }
            });
        });

    if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.preset_switch_confirm = None;
    }
}

fn render_overwrite_confirm(ctx: &egui::Context, app: &mut PanteonApp) {
    let Some(name) = app.preset_overwrite_confirm.clone() else {
        return;
    };
    let mut close = false;

    egui::Window::new("Перезаписать профиль")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!("Перезаписать профиль „{}“?", name))
                    .strong()
                    .size(15.0),
            );
            ui.add_space(8.0);
            ui.label("Сохранённая конфигурация будет заменена текущей.");
            ui.label(
                egui::RichText::new("Это действие нельзя отменить.")
                    .color(theme::TEXT_SECONDARY),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Отмена").clicked() {
                    close = true;
                }
                let confirm = egui::Button::new(
                    egui::RichText::new("Перезаписать")
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(theme::DANGER);
                if ui.add(confirm).clicked() {
                    overwrite_preset(app, &name);
                    close = true;
                }
            });
        });

    if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.preset_overwrite_confirm = None;
    }
}

fn render_delete_confirm(ctx: &egui::Context, app: &mut PanteonApp) {
    let Some(name) = app.preset_delete_confirm.clone() else {
        return;
    };
    let mut close = false;

    egui::Window::new("Удалить профиль")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!("Удалить профиль „{}“?", name))
                    .strong()
                    .size(15.0),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Файл будет удалён безвозвратно.")
                    .color(theme::TEXT_SECONDARY),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Отмена").clicked() {
                    close = true;
                }
                let confirm = egui::Button::new(
                    egui::RichText::new("Удалить")
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(theme::DANGER);
                if ui.add(confirm).clicked() {
                    delete_preset(app, &name);
                    close = true;
                }
            });
        });

    if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.preset_delete_confirm = None;
    }
}

pub(crate) fn load_profile_into_app(app: &mut PanteonApp, prof: &Profile) {
    app.selected_polling = prof.polling_rate.as_hz();
    app.selected_dpi_slot = prof.active_dpi_index;
    app.fire_delay = prof.fire_delay_ms;
    app.led_speed = prof.led_speed;
    // Idempotent: also normalizes profiles that reach the app by other paths
    // (import, future entry points) — level 0 is wire 100% (live observation).
    app.led_brightness = crate::codec::normalize_brightness_level(prof.led_brightness);
    app.all_colors = prof.led_all_colors;

    // Map LED effect string to option index
    for (i, opt) in crate::gui::LED_EFFECT_OPTIONS.iter().enumerate() {
        if opt.label == prof.led_effect {
            app.selected_effect_idx = i;
            break;
        }
    }

    if prof.dpi_levels.len() >= 6 {
        for i in 0..6 {
            app.dpi_slots[i] = prof.dpi_levels[i].dpi as u32;
            app.slot_colors[i] = prof.dpi_levels[i].main;
        }
    }
}

pub(crate) fn build_profile_from_app(app: &PanteonApp) -> Profile {
    let dpi_levels = (0..6)
        .map(|i| crate::model::DpiSlot {
            level_index: (i + 1) as u8,
            dpi: app.dpi_slots[i] as u16,
            enabled: true,
            main: app.slot_colors[i],
            second: crate::model::DEFAULT_SECOND_COLORS[i],
        })
        .collect();

    Profile {
        polling_rate: crate::model::PollingRate::from_hz(app.selected_polling)
            .unwrap_or(crate::model::PollingRate::Hz1000),
        active_dpi_index: app.selected_dpi_slot,
        fire_delay_ms: app.fire_delay,
        led_speed: app.led_speed,
        led_brightness: app.led_brightness,
        led_all_colors: app.all_colors,
        led_effect: crate::gui::LED_EFFECT_OPTIONS[app.selected_effect_idx].label.to_string(),
        dpi_levels,
        ..Default::default()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn stale_selection_guard() {
        // No selection: nothing to resolve.
        let mut selected: Option<String> = None;
        assert!(!resolve_stale_selection(&mut selected, &names(&["base1"])));
        assert_eq!(selected, None);

        // Existing profile: the selection survives the per-frame check.
        let mut selected = Some("base1".to_string());
        assert!(!resolve_stale_selection(&mut selected, &names(&["base1", "base2"])));
        assert_eq!(selected.as_deref(), Some("base1"));

        // File deleted outside the app: selection is dropped, no panic.
        let mut selected = Some("base1".to_string());
        assert!(resolve_stale_selection(&mut selected, &names(&["base2"])));
        assert_eq!(selected, None);

        // Directory emptied/deleted: also a quiet reset.
        let mut selected = Some("base1".to_string());
        assert!(resolve_stale_selection(&mut selected, &[]));
        assert_eq!(selected, None);
    }

    #[test]
    fn new_profile_state_never_allows_overwrite() {
        // Invalid names carry the validate_name reason for the tooltip.
        assert_eq!(
            new_profile_state(presets::validate_name(""), false),
            NewProfileState::Invalid("Имя профиля не может быть пустым".to_string())
        );
        assert!(matches!(
            new_profile_state(presets::validate_name("../evil"), false),
            NewProfileState::Invalid(msg) if msg.contains("«..»")
        ));
        // Valid but already saved: creation is blocked, never a silent overwrite.
        assert_eq!(
            new_profile_state(presets::validate_name("base1"), true),
            NewProfileState::NameTaken
        );
        // Valid and free: creation allowed.
        assert_eq!(
            new_profile_state(presets::validate_name("base2"), false),
            NewProfileState::Ready
        );
    }

    #[test]
    fn name_taken_reason_mentions_both_ways_out() {
        assert!(NAME_TAKEN_REASON.contains("Имя занято"));
        assert!(NAME_TAKEN_REASON.contains("обновите существующий профиль"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn which_and_file_manager_resolve_on_path() {
        assert!(which("sh").is_some(), "sh must be on PATH");
        assert!(which("definitely-not-a-real-binary-xyz").is_none());
        if let Some(program) = file_manager_program() {
            assert!(which(program).is_some(), "{program} must resolve on PATH");
        }
    }

    #[test]
    fn selector_placeholder_text() {
        assert_eq!(selector_placeholder(&[]), "Нет профилей");
        assert_eq!(selector_placeholder(&names(&["base1"])), "Выберите профиль…");
        // With a selection the placeholder is not used at all.
        assert_eq!(selector_width(), 260.0);
    }

    /// Two profiles that differ only in the fields the editor does not represent
    /// must compare *equal*: this is the false-«modified» guard.
    #[test]
    fn profile_config_eq_ignores_non_editor_fields() {
        use crate::model::{ButtonAction, ButtonBinding};

        let a = Profile::factory_default();
        let mut b = a.clone();
        b.id = a.id.wrapping_add(1);
        b.name = "Другое имя".to_string();
        b.button_bindings = vec![ButtonBinding {
            button_index: 9,
            action: ButtonAction::LeftClick,
        }];
        for slot in &mut b.dpi_levels {
            slot.second = [1, 2, 3];
        }

        // The naive whole-struct comparison *would* fire here; ours must not.
        assert_ne!(a, b, "fixture must actually differ outside the editor fields");
        assert!(
            profile_config_eq(&a, &b),
            "id/name/button_bindings/second are not editor state"
        );
        assert!(profile_config_eq(&a, &a));
        // And the relation is symmetric.
        assert!(profile_config_eq(&b, &a));
    }

    fn assert_modified(label: &str, mutate: impl FnOnce(&mut Profile)) {
        let a = Profile::factory_default();
        let mut b = a.clone();
        mutate(&mut b);
        assert!(
            !profile_config_eq(&a, &b),
            "{label} must be detected as a configuration change"
        );
    }

    #[test]
    fn profile_config_eq_detects_every_compared_field() {
        use crate::model::PollingRate;

        assert_modified("active_dpi_index", |p| p.active_dpi_index = 3);
        assert_modified("polling_rate", |p| p.polling_rate = PollingRate::Hz250);
        assert_modified("fire_delay_ms", |p| p.fire_delay_ms = 120);
        assert_modified("led_effect", |p| p.led_effect = "6. Комета".to_string());
        assert_modified("led_speed", |p| p.led_speed = 5);
        assert_modified("led_brightness", |p| p.led_brightness = 0);
        assert_modified("led_all_colors", |p| p.led_all_colors = !p.led_all_colors);
        assert_modified("slot level_index", |p| p.dpi_levels[0].level_index = 7);
        assert_modified("slot enabled", |p| p.dpi_levels[0].enabled = false);
        assert_modified("slot dpi (first)", |p| p.dpi_levels[0].dpi = 800);
        assert_modified("slot main colour (last)", |p| {
            p.dpi_levels[5].main = [1, 2, 3]
        });

        // Every slot is actually compared (not just the first one).
        for i in 0..6 {
            let a = Profile::factory_default();
            let mut b = a.clone();
            b.dpi_levels[i].dpi = a.dpi_levels[i].dpi.wrapping_add(23);
            assert!(!profile_config_eq(&a, &b), "slot {i}: dpi change missed");

            let mut c = a.clone();
            c.dpi_levels[i].main = [9, 8, 7];
            assert!(!profile_config_eq(&a, &c), "slot {i}: main colour change missed");
        }
    }

    #[test]
    fn profile_config_eq_rejects_truncated_level_lists() {
        let full = Profile::factory_default();

        let mut short = full.clone();
        short.dpi_levels.pop();
        assert!(!profile_config_eq(&full, &short));

        let mut empty = full.clone();
        empty.dpi_levels.clear();
        assert!(!profile_config_eq(&full, &empty));
        assert!(!profile_config_eq(&empty, &full));

        // Even identical truncated profiles are not comparable.
        assert!(!profile_config_eq(&short, &short.clone()));
    }

    /// R20: a profile file carrying wire brightness 0 (= 100% on hardware) must
    /// normalize *before* both the editor and the R18 snapshot — otherwise the
    /// loaded profile instantly reports a false «profile modified».
    #[test]
    fn profile_with_wire_brightness_zero_loads_as_100_and_indicator_stays_quiet() {
        // Build the fixture from the default editor state (catalog labels, slot
        // layout), then rewind brightness to the legacy wire value 0.
        let mut fixture = build_profile_from_app(&PanteonApp::default());
        fixture.led_brightness = 0;
        assert_eq!(fixture.led_brightness, 0, "fixture must carry wire level 0");

        let dir = std::env::temp_dir().join(format!(
            "panteon-ms89-r20-brightness0-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create fixture presets dir");
        presets::save(&dir, "r20_zero", &fixture).expect("save brightness-0 fixture");

        let mut app = PanteonApp::default();
        load_preset_into_editor(&mut app, &dir, "r20_zero");

        assert_eq!(app.led_brightness, 8, "wire 0 must load as editor 100%");
        let snapshot = app
            .selected_preset_snapshot
            .as_ref()
            .expect("load takes a saved-state snapshot");
        assert_eq!(
            snapshot.led_brightness, 8,
            "the snapshot must be normalized as well"
        );
        assert!(
            profile_config_eq(&build_profile_from_app(&app), snapshot),
            "R18 indicator must stay quiet for a brightness = 0 profile"
        );

        std::fs::remove_dir_all(&dir).expect("cleanup fixture presets dir");
    }
}
