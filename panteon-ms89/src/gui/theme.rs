use std::sync::Arc;
use eframe::egui::{self, Color32, Pos2, Rect, Shape};

// Цвета
pub const BG: Color32 = Color32::from_rgb(0x16, 0x16, 0x18);
pub const BG_GRAD_TOP: Color32 = Color32::from_rgb(0x1a, 0x1b, 0x2e);
pub const BG_GRAD_BOTTOM: Color32 = Color32::from_rgb(0x16, 0x16, 0x18);
pub const RAIL_BG: Color32 = Color32::from_rgb(0x13, 0x13, 0x15);

// Карточка: #202126 с альфой 200 (~0.80..0.85)
pub const CARD: Color32 = Color32::from_rgba_premultiplied(25, 26, 30, 200);
/// Opaque equivalent of [`CARD`] over the panel gradient — for *elevated* surfaces
/// (dropdown popups, modal windows) that cover other content: a translucent popup
/// let the card text underneath bleed through and made menu items unreadable.
pub const CARD_OPAQUE: Color32 = Color32::from_rgb(0x1e, 0x1f, 0x25);
pub const CARD_STROKE: Color32 = Color32::from_rgb(0x2e, 0x2f, 0x3a);

pub const ACCENT: Color32 = Color32::from_rgb(0x0a, 0x84, 0xff);
// Свечение: #0A84FF с альфой 0.25 (64/255)
pub const ACCENT_GLOW: Color32 = Color32::from_rgba_premultiplied(2, 33, 64, 64);

pub const SUCCESS: Color32 = Color32::from_rgb(0x30, 0xd1, 0x58);
pub const WARN: Color32 = Color32::from_rgb(0xff, 0x9f, 0x0a);
pub const DANGER: Color32 = Color32::from_rgb(0xff, 0x45, 0x3a);
pub const INFO: Color32 = Color32::from_rgb(0x34, 0x98, 0xdb);

pub const TEXT: Color32 = Color32::from_rgb(0xf5, 0xf5, 0xf7);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x98, 0x98, 0x9f);
pub const SEPARATOR: Color32 = Color32::from_rgb(0x2e, 0x2f, 0x3a);
pub const HOVER_BG: Color32 = Color32::from_rgba_premultiplied(44, 46, 56, 220);

// Размеры
pub const RAIL_W: f32 = 64.0;
pub const NAV_BTN: f32 = 44.0;
pub const NAV_RADIUS: f32 = 12.0;
pub const CARD_R: f32 = 14.0;
pub const BTN_H: f32 = 36.0;
pub const PILL_H: f32 = 32.0;
pub const GAP: f32 = 16.0;
pub const CARD_PAD: f32 = 16.0;
pub const CONTENT_W: f32 = 820.0;

// Тултипы: узкая колонка (переносится, не обрезается краем окна), короткая задержка.
pub const TOOLTIP_W: f32 = 260.0;
pub const TOOLTIP_DELAY: f32 = 0.25;
pub const TOOLTIP_GRACE: f32 = 0.15;
pub const TOOLTIP_TEXT_SIZE: f32 = 12.5;

/// Единый стиль подсказок: тёмная поверхность CARD, radius 8 (menu_corner_radius),
/// текст 12.5, перенос по TOOLTIP_W. Используйте вместо голого `on_hover_text`,
/// чтобы длинные двухстрочные подсказки не уезжали за край окна.
pub fn tooltip(resp: egui::Response, text: impl Into<String>) -> egui::Response {
    if !resp.enabled() {
        return resp;
    }
    resp.on_hover_ui(|ui| {
        ui.set_max_width(TOOLTIP_W);
        ui.label(
            egui::RichText::new(text.into())
                .size(TOOLTIP_TEXT_SIZE)
                .color(TEXT),
        );
    })
}

/// Рисует вертикальный градиент фона BG_GRAD_TOP -> BG_GRAD_BOTTOM с помощью 4-вершинного Mesh.
pub fn paint_background_gradient(painter: &egui::Painter, rect: Rect) {
    let mut mesh = egui::Mesh::default();
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.left_top(),
        uv: egui::epaint::WHITE_UV,
        color: BG_GRAD_TOP,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.right_top(),
        uv: egui::epaint::WHITE_UV,
        color: BG_GRAD_TOP,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.left_bottom(),
        uv: egui::epaint::WHITE_UV,
        color: BG_GRAD_BOTTOM,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.right_bottom(),
        uv: egui::epaint::WHITE_UV,
        color: BG_GRAD_BOTTOM,
    });
    mesh.indices.extend([0, 1, 2, 1, 3, 2]);
    painter.add(Shape::Mesh(Arc::new(mesh)));
}

/// Рисует мягкое радиальное свечение в правом верхнем углу CentralPanel.
/// Сниженная яркость (макс. альфа 25) для предотвращения доминирования пятна в узких окнах.
pub fn paint_accent_glow(painter: &egui::Painter, rect: Rect) {
    let center = Pos2::new(rect.right() - 80.0, rect.top() + 60.0);
    // 5 вложенных circle_filled с плавно падающей альфой (макс. альфа 25)
    let layers = [
        (220.0, Color32::from_rgba_premultiplied(0, 3, 6, 6)),
        (160.0, Color32::from_rgba_premultiplied(0, 5, 10, 10)),
        (110.0, Color32::from_rgba_premultiplied(1, 8, 15, 15)),
        (70.0, Color32::from_rgba_premultiplied(1, 10, 20, 20)),
        (35.0, Color32::from_rgba_premultiplied(1, 13, 25, 25)),
    ];
    for (radius, color) in layers {
        painter.circle_filled(center, radius, color);
    }
}

/// Apple-style quiet button: CARD surface, CARD_STROKE border, native hover
/// (visuals + `Response::hovered()`), white text on hover.
/// `PANTEON_HOVER=<id_salt>` forces the hover *look* for screenshots — it only
/// swaps the idle visuals, it never fakes input, so real tooltips stay real.
pub fn quiet_pill_button(
    ui: &mut egui::Ui,
    id_salt: &str,
    text: &str,
    tooltip_text: &str,
) -> egui::Response {
    let env_hover = std::env::var("PANTEON_HOVER").unwrap_or_default();
    let forced_hover = env_hover == id_salt;

    let resp = ui
        .scope(|ui| {
            let (idle_fill, idle_text) = if forced_hover {
                (HOVER_BG, egui::Color32::WHITE)
            } else {
                (CARD, TEXT_SECONDARY)
            };
            let visuals = ui.visuals_mut();
            visuals.widgets.inactive.weak_bg_fill = idle_fill;
            visuals.widgets.inactive.bg_fill = idle_fill;
            visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, idle_text);
            visuals.widgets.hovered.weak_bg_fill = HOVER_BG;
            visuals.widgets.hovered.bg_fill = HOVER_BG;
            visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
            visuals.widgets.active.weak_bg_fill = HOVER_BG;
            visuals.widgets.active.bg_fill = HOVER_BG;
            visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);

            let btn = egui::Button::new(egui::RichText::new(text).size(13.0))
                .min_size(egui::vec2(80.0, PILL_H))
                .corner_radius(egui::CornerRadius::same(8))
                .stroke(egui::Stroke::new(1.0, CARD_STROKE));
            ui.add(btn)
        })
        .inner;

    let resp = tooltip(resp, tooltip_text);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Подсказка для выключенного виджета (та же поверхность/типографика).
pub fn disabled_tooltip(resp: egui::Response, text: impl Into<String>) -> egui::Response {
    let text = text.into();
    resp.on_disabled_hover_ui(|ui| {
        ui.set_max_width(TOOLTIP_W);
        ui.label(
            egui::RichText::new(text)
                .size(TOOLTIP_TEXT_SIZE)
                .color(TEXT),
        );
    })
}

pub fn nav_active_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(ACCENT)
        .corner_radius(egui::CornerRadius::same(NAV_RADIUS as u8))
        .shadow(egui::Shadow {
            offset: [0, 0],
            blur: 16,
            spread: 0,
            color: ACCENT_GLOW,
        })
}

