//! Visual style: dark rounded theme (same family as PadTune), cards, choice chips and a toggle.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontFamily, RichText, Stroke};
use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};

pub const ACCENT: Color32 = Color32::from_rgb(0x7c, 0x5c, 0xff);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0x2a, 0x24, 0x4a);
pub const BG: Color32 = Color32::from_rgb(0x12, 0x13, 0x18);
pub const PANEL: Color32 = Color32::from_rgb(0x19, 0x1b, 0x22);
pub const CARD: Color32 = Color32::from_rgb(0x21, 0x23, 0x2c);
pub const CONTROL: Color32 = Color32::from_rgb(0x2a, 0x2d, 0x38);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9a, 0x9e, 0xb0);
pub const OK: Color32 = Color32::from_rgb(0x3d, 0xd6, 0x8c);
pub const WARN: Color32 = Color32::from_rgb(0xff, 0xb4, 0x4d);
pub const ERR: Color32 = Color32::from_rgb(0xff, 0x5c, 0x6c);

/// Width of the label column inside cards.
pub const LABEL_WIDTH: f32 = 170.0;

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = CARD;
    v.extreme_bg_color = BG;
    v.faint_bg_color = CARD;
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = ACCENT;
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(8);
    }
    v.widgets.inactive.weak_bg_fill = CONTROL;
    v.widgets.inactive.bg_fill = CONTROL;
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x34, 0x37, 0x45);
    v.widgets.active.weak_bg_fill = ACCENT;
    v.window_corner_radius = CornerRadius::same(12);
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.all_styles_mut(|s| {
        use egui::{FontId, TextStyle};
        s.spacing.item_spacing = egui::vec2(10.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.slider_width = 280.0;
        s.spacing.interact_size.y = 26.0;
        s.text_styles
            .insert(TextStyle::Heading, FontId::proportional(22.0));
        s.text_styles
            .insert(TextStyle::Body, FontId::proportional(14.5));
        s.text_styles
            .insert(TextStyle::Button, FontId::proportional(14.5));
        s.text_styles
            .insert(TextStyle::Small, FontId::proportional(12.0));
    });
    install_fallback_fonts(ctx);
}

/// System fonts that cover the symbols egui's bundled fonts lack (sidebar icons, arrows, dots).
fn install_fallback_fonts(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
        "/usr/share/fonts/google-noto/NotoSansSymbols2-Regular.ttf",
        "C:\\Windows\\Fonts\\seguisym.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
    ];
    for path in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            ctx.add_font(FontInsert::new(
                "symbols-fallback",
                FontData::from_owned(bytes),
                vec![
                    InsertFontFamily {
                        family: FontFamily::Proportional,
                        priority: FontPriority::Lowest,
                    },
                    InsertFontFamily {
                        family: FontFamily::Monospace,
                        priority: FontPriority::Lowest,
                    },
                ],
            ));
            return;
        }
    }
}

/// A rounded card with a title and an optional one-line explanation.
pub fn card(ui: &mut egui::Ui, title: &str, hint: Option<&str>, add: impl FnOnce(&mut egui::Ui)) {
    // The margin is added around the content, so reserve it up front.
    let width = (ui.available_width() - 32.0).max(0.0);
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.label(
                RichText::new(title)
                    .strong()
                    .size(15.5)
                    .color(Color32::WHITE),
            );
            if let Some(hint) = hint {
                ui.label(dim(hint));
            }
            ui.add_space(6.0);
            add(ui);
        });
    ui.add_space(12.0);
}

/// Label column + control, on one line.
pub fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        let size = egui::vec2(LABEL_WIDTH, ui.spacing().interact_size.y);
        ui.allocate_ui_with_layout(
            size,
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_size(size);
                ui.label(RichText::new(label).color(TEXT_DIM));
            },
        );
        add(ui);
    });
}

/// Small grey caption.
pub fn dim(text: impl Into<String>) -> RichText {
    RichText::new(text).color(TEXT_DIM).small()
}

/// One selectable chip; returns true when clicked.
pub fn chip(ui: &mut egui::Ui, selected: bool, text: &str) -> bool {
    let label = RichText::new(text).color(if selected { Color32::WHITE } else { TEXT_DIM });
    let button = egui::Button::new(label)
        .fill(if selected { ACCENT } else { CONTROL })
        .corner_radius(CornerRadius::same(14));
    ui.add(button).clicked()
}

/// iOS-style switch.
pub fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = ui.spacing().interact_size.y * egui::vec2(1.9, 1.0);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(response.id, *on);
        let radius = 0.5 * rect.height();
        let fill = if *on { ACCENT } else { CONTROL };
        let fill = if ui.is_enabled() {
            fill
        } else {
            fill.gamma_multiply(0.5)
        };
        ui.painter()
            .rect(rect, radius, fill, Stroke::NONE, egui::StrokeKind::Inside);
        let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
        ui.painter().circle_filled(
            egui::pos2(x, rect.center().y),
            0.75 * radius,
            Color32::WHITE,
        );
    }
    response
}
