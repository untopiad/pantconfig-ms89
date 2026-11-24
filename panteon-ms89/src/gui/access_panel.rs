//! R23: access-diagnostics presentation for the GUI (status-bar chip + modal).
//!
//! The mapping from [`AccessReport`] to user-visible strings is pure and covered
//! by unit tests; [`render_access_modal`] only renders it. The diagnostics
//! themselves live in [`crate::device::access`] and are read-only.

use eframe::egui;

use crate::device::{
    access, default_rule_dirs, report_is_ok, rule_dirs_from_env, AccessReport, NodeAccess,
    RuleState,
};
use crate::gui::app::PanteonApp;
use crate::gui::theme;

/// Status-bar chip label shown when access is incomplete.
pub const CHIP_TEXT: &str = "Нет доступа к устройству";

/// Two-sentence explanation: what the rule is for, and why `input` is not needed.
pub const EXPLANATION: &str = "Правило udev выдаёт вашей сессии ACL на /dev/hidraw* \
(конфигурация мыши) и на USB-устройство (/dev/bus/usb/…), нужное для USB-reset после \
смены частоты опроса. Группа input не требуется: она шире необходимого и не покрывает \
USB-reset.";

/// Whether the warning chip is shown. An absent report (before the first scan)
/// is not a warning.
pub fn chip_visible(report: Option<&AccessReport>) -> bool {
    report.map(|r| !report_is_ok(r)).unwrap_or(false)
}

/// Tooltip for the chip: the concrete failing parts, not a generic message.
pub fn chip_tooltip(report: &AccessReport) -> String {
    let mut reasons: Vec<&str> = Vec::new();
    match report.rule {
        RuleState::Missing => reasons.push("udev-правило не установлено"),
        RuleState::Outdated => reasons.push("udev-правило устарело"),
        RuleState::Installed => {}
    }
    if report.hidraw == NodeAccess::Denied {
        reasons.push("hidraw: нет доступа");
    }
    if report.usb_reset == NodeAccess::Denied {
        reasons.push("USB reset: нет доступа");
    }

    if reasons.is_empty() {
        "Доступ к устройству: в порядке.".to_string()
    } else {
        format!("{}\n• {}", CHIP_TEXT, reasons.join("\n• "))
    }
}

/// Human line for the udev rule state.
pub fn rule_line(report: &AccessReport) -> String {
    let path = report
        .rule_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "путь неизвестен".to_string());
    match report.rule {
        RuleState::Installed => format!("udev-правило: установлено ({})", path),
        RuleState::Outdated => format!(
            "udev-правило: устарело ({}) — содержимое отличается от поставляемого",
            path
        ),
        RuleState::Missing => {
            let searched = rule_dirs_from_env().unwrap_or_else(default_rule_dirs);
            let searched = searched
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("udev-правило: отсутствует (искали: {})", searched)
        }
    }
}

/// Human line for one device node.
pub fn node_line(label: &str, access: NodeAccess) -> String {
    let state = match access {
        NodeAccess::Ok => "доступ есть",
        NodeAccess::Denied => "нет доступа",
        NodeAccess::NoNode => "устройство не подключено",
    };
    format!("{}: {}", label, state)
}

/// Pure view-model of the access modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessModalView {
    pub title: &'static str,
    pub lines: Vec<String>,
    pub explanation: &'static str,
    pub install_command: String,
    pub is_ok: bool,
}

pub fn build_modal_view(report: &AccessReport) -> AccessModalView {
    AccessModalView {
        title: "Доступ к устройству",
        lines: vec![
            rule_line(report),
            node_line("hidraw", report.hidraw),
            node_line("USB reset", report.usb_reset),
        ],
        explanation: EXPLANATION,
        install_command: access::install_command(),
        is_ok: report_is_ok(report),
    }
}

/// Renders the modal when open. Recheck is explicit (button) — no per-frame I/O.
pub fn render_access_modal(ctx: &egui::Context, app: &mut PanteonApp) {
    if !app.access_modal_open {
        return;
    }
    let Some(report) = app.access_report.clone() else {
        app.access_modal_open = false;
        return;
    };

    let view = build_modal_view(&report);
    let mut close = false;
    let mut copy = false;
    let mut recheck = false;
    let mut content_rect: Option<egui::Rect> = None;

    let window = egui::Window::new(view.title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_width(560.0)
        .max_width(660.0)
        .show(ctx, |ui| {
            for line in &view.lines {
                ui.label(egui::RichText::new(line).color(theme::TEXT));
            }

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(view.explanation)
                    .size(12.5)
                    .color(theme::TEXT_SECONDARY),
            );

            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("Команда установки (одноразово, нужен sudo):")
                    .strong()
                    .color(theme::TEXT),
            );
            egui::Frame::new()
                .fill(theme::CARD_OPAQUE)
                .stroke(egui::Stroke::new(1.0, theme::CARD_STROKE))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&view.install_command)
                                .monospace()
                                .size(11.5),
                        )
                        .selectable(true),
                    );
                });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Скопировать команду").clicked() {
                    copy = true;
                }
                if ui.button("Перепроверить").clicked() {
                    recheck = true;
                }
                if ui.button("Закрыть").clicked() {
                    close = true;
                }
            });
            content_rect = Some(ui.min_rect());
        });

    if copy {
        // `PlatformOutput::copied_text` is a no-op in eframe 0.31 native
        // (only `OutputCommand`s are processed); `Context::copy_text` emits
        // the same CopyText command and actually fills the clipboard.
        ctx.copy_text(view.install_command.clone());
    }

    if recheck {
        let fresh = access::diagnose(&app.devices);
        let ok = report_is_ok(&fresh);
        app.access_report = Some(fresh);
        if ok {
            close = true;
        }
    }

    // Click-outside closes (Esc/«Закрыть» too, R16/R17 pattern). We use the
    // content rect captured *after* the button row: egui's `Window` response
    // rect excludes trailing widgets, so `Response::clicked_elsewhere()` would
    // fire on clicks visually inside the dialog (verified live).
    let _ = window;
    if ctx.input(|i| i.pointer.any_click()) {
        if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
            if content_rect.map(|r| !r.contains(pos)).unwrap_or(false) {
                close = true;
            }
        }
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    if close {
        app.access_modal_open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::AccessReport;

    fn report(rule: RuleState, hidraw: NodeAccess, usb: NodeAccess) -> AccessReport {
        AccessReport {
            rule,
            rule_path: Some(std::path::PathBuf::from("/etc/udev/rules.d/99-panteon-ms89.rules")),
            hidraw,
            usb_reset: usb,
        }
    }

    fn all_reports() -> Vec<AccessReport> {
        let rules = [
            RuleState::Missing,
            RuleState::Outdated,
            RuleState::Installed,
        ];
        let nodes = [NodeAccess::Ok, NodeAccess::Denied, NodeAccess::NoNode];
        let mut out = Vec::new();
        for rule in rules {
            for hidraw in nodes {
                for usb in nodes {
                    let mut r = report(rule, hidraw, usb);
                    if rule == RuleState::Missing {
                        r.rule_path = None;
                    }
                    out.push(r);
                }
            }
        }
        out
    }

    #[test]
    fn chip_visibility_matches_report_is_ok_for_every_combination() {
        assert!(!chip_visible(None));
        for r in all_reports() {
            assert_eq!(
                chip_visible(Some(&r)),
                !report_is_ok(&r),
                "chip visibility must mirror report_is_ok for {r:?}"
            );
        }
    }

    #[test]
    fn chip_tooltip_names_every_failing_part() {
        let r = report(RuleState::Missing, NodeAccess::Denied, NodeAccess::Denied);
        let tip = chip_tooltip(&r);
        assert!(tip.starts_with(CHIP_TEXT));
        assert!(tip.contains("udev-правило не установлено"));
        assert!(tip.contains("hidraw: нет доступа"));
        assert!(tip.contains("USB reset: нет доступа"));

        let outdated = report(RuleState::Outdated, NodeAccess::Ok, NodeAccess::Denied);
        let tip = chip_tooltip(&outdated);
        assert!(tip.contains("udev-правило устарело"));
        assert!(!tip.contains("hidraw: нет доступа"));
        assert!(tip.contains("USB reset: нет доступа"));

        let ok = report(
            RuleState::Installed,
            NodeAccess::Ok,
            NodeAccess::NoNode,
        );
        assert!(!chip_visible(Some(&ok)));
        assert_eq!(chip_tooltip(&ok), "Доступ к устройству: в порядке.");
    }

    #[test]
    fn modal_view_lines_cover_rule_and_both_nodes() {
        let r = report(RuleState::Outdated, NodeAccess::Denied, NodeAccess::NoNode);
        let view = build_modal_view(&r);
        assert_eq!(view.title, "Доступ к устройству");
        assert_eq!(view.lines.len(), 3);
        assert!(view.lines[0].contains("устарело"));
        assert!(view.lines[1].starts_with("hidraw:"));
        assert!(view.lines[2].starts_with("USB reset:"));
        assert!(!view.is_ok);
        assert!(view.install_command.contains("sudo tee"));
        assert!(view.install_command.contains(access::UDEV_RULE.trim_end()));
        assert_eq!(view.explanation, EXPLANATION);
    }

    #[test]
    fn modal_view_marks_installed_rule_and_open_nodes_as_ok() {
        let r = report(RuleState::Installed, NodeAccess::Ok, NodeAccess::Ok);
        let view = build_modal_view(&r);
        assert!(view.is_ok);
        assert!(view.lines[0].contains("установлено"));
        assert!(view.lines[1].contains("доступ есть"));
        assert!(view.lines[2].contains("доступ есть"));
    }

    /// R23 regression: diagnostics are UI-only and must never participate in
    /// dirty tracking. Mutating them and re-syncing leaves `AppBaseline` intact.
    #[test]
    fn access_report_is_outside_app_baseline() {
        let mut app = PanteonApp::default();
        let before = app.baseline.clone();
        app.access_report = Some(report(
            RuleState::Missing,
            NodeAccess::Denied,
            NodeAccess::Denied,
        ));
        app.access_modal_open = true;
        app.sync_baseline();
        assert_eq!(app.baseline, before);
        assert_eq!(app.dirty_categories(), Vec::new());
    }
}