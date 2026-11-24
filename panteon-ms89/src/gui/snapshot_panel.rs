use std::path::{Path, PathBuf};
use eframe::egui;

use crate::gui::app::PanteonApp;

/// Resolves snapshot file path by checking:
/// 1. Direct path (if absolute and exists)
/// 2. Relative to current working directory (`cwd`)
/// 3. Relative to parent of `cwd` (`../<path>`)
pub fn resolve_snapshot_path(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("Путь к файлу снапшота пуст. Используйте кнопку 'Обзор...' для выбора файла.".to_string());
    }

    if path.is_absolute() {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        return Err(format!(
            "Абсолютный путь не существует: '{}'. Используйте кнопку 'Обзор...'",
            path.display()
        ));
    }

    if let Ok(cwd) = std::env::current_dir() {
        let p_cwd = cwd.join(path);
        if p_cwd.exists() {
            return Ok(p_cwd);
        }

        if let Some(parent) = cwd.parent() {
            let p_parent = parent.join(path);
            if p_parent.exists() {
                return Ok(p_parent);
            }
        }
    }

    Err(format!(
        "Файл не найден (проверено в cwd и ../): '{}'. Используйте кнопку 'Обзор...' для выбора файла.",
        path.display()
    ))
}

/// Opens native file chooser dialog using `rfd`
#[cfg(feature = "gui")]
pub fn pick_snapshot_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select PANTEON MS89 State Snapshot")
        .add_filter("Snapshot files (*.pcap, *.pcapng, *.toml)", &["pcap", "pcapng", "toml"])
        .add_filter("All files", &["*"])
        .pick_file()
}

pub fn render_snapshot_panel(ui: &mut egui::Ui, app: &mut PanteonApp) {
    ui.horizontal(|ui| {
        if ui.button("Выбрать снимок").clicked() {
            if let Some(picked) = pick_snapshot_file() {
                app.snapshot_path = picked.to_string_lossy().to_string();
                app.snapshot_error = None;
                app.load_snapshot();
            }
        }
        if !app.snapshot_path.is_empty() {
            ui.label(
                egui::RichText::new(&app.snapshot_path)
                    .small()
                    .color(crate::gui::theme::TEXT_SECONDARY),
            );
        } else {
            ui.label(
                egui::RichText::new("Снимок не выбран")
                    .small()
                    .weak(),
            );
        }
    });

    ui.add_space(4.0);

    // Local error banner displayed in RED directly inside the panel
    if let Some(err) = &app.snapshot_error {
        ui.colored_label(crate::gui::theme::DANGER, format!("Ошибка: {}", err));
    }

    if let Some(snap) = &app.snapshot_preview {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Сведения о загруженном снимке:").strong());
            ui.label(format!("Метка: {}", snap.label));
            ui.label(format!("Исходный PID: 0x{:04x}", snap.source_pid));
            ui.label(format!("Число кадров: {}", snap.frames.len()));

            if let Some(dev) = app.current_device() {
                if dev.product_id != snap.source_pid {
                    ui.colored_label(
                        crate::gui::theme::WARN,
                        format!(
                            "Предупреждение: PID снимка (0x{:04x}) не совпадает с PID подключённого устройства (0x{:04x})!",
                            snap.source_pid, dev.product_id
                        ),
                    );
                }
            }
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if ui.button("Восстановить заводские настройки").clicked() {
                app.request_factory_reset();
            }
        });
    } else {
        ui.horizontal(|ui| {
            if ui.button("Восстановить заводские настройки").clicked() {
                app.request_factory_reset();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_snapshot_path_missing() {
        let missing = Path::new("completely_nonexistent_snapshot_file_99999.pcap");
        let res = resolve_snapshot_path(missing);
        assert!(res.is_err());
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains("Файл не найден"));
        assert!(err_msg.contains("Обзор"));
    }

    #[test]
    fn test_resolve_snapshot_path_empty() {
        let empty = Path::new("");
        let res = resolve_snapshot_path(empty);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("пуст"));
    }

    #[test]
    fn test_resolve_snapshot_path_absolute() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_snap_abs_tmp.toml");
        std::fs::write(&temp_file, "dummy content").unwrap();

        let resolved = resolve_snapshot_path(&temp_file).expect("Absolute path must resolve");
        assert_eq!(resolved, temp_file);

        let _ = std::fs::remove_file(temp_file);
    }

    #[test]
    fn test_resolve_snapshot_path_cwd_and_parent() {
        let cwd = std::env::current_dir().unwrap();
        let rel_file = "Cargo.toml";
        if cwd.join(rel_file).exists() {
            let res = resolve_snapshot_path(Path::new(rel_file));
            assert!(res.is_ok());
            assert_eq!(res.unwrap(), cwd.join(rel_file));
        }

        // Test with fixture path
        let fixture_rel = Path::new("tests/fixtures/breath_effect.pcapng");
        let res_fixture = resolve_snapshot_path(fixture_rel);
        // Either cwd or parent should find it if repository root or crate root
        if Path::new("tests/fixtures/breath_effect.pcapng").exists()
            || Path::new("../tests/fixtures/breath_effect.pcapng").exists()
        {
            assert!(res_fixture.is_ok());
        }
    }
}
