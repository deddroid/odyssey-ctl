//! Visual style: light ("Carta calda") and dark ("Viola notte") palettes following the system
//! theme, Inter for text and Rajdhani for titles, and the widgets the pages are built from:
//! grouped lists of rows with the control on the right, option menus, sliders and switches.
//!
//! Corner radii are concentric: capsules for small controls, 12 for rows and menu items,
//! 16 for groups and menus (12 + their 4 px padding), 20 for windows.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontFamily, FontId, Response, RichText, Stroke,
};
use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};

/// Colours of one theme.
pub struct Palette {
    /// Window and group background.
    pub bg: Color32,
    /// Page surface the groups sit on.
    pub surface: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    /// The single accent: selection, active values, primary action.
    pub accent: Color32,
    /// Background of unfilled controls: slider rail, switch off, value capsules.
    pub track: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub err: Color32,
}

/// "Viola notte".
pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x15, 0x15, 0x1c),
    surface: Color32::from_rgb(0x1e, 0x1e, 0x28),
    text: Color32::from_rgb(0xec, 0xec, 0xf3),
    text_dim: Color32::from_rgb(0x8b, 0x8b, 0xa0),
    accent: Color32::from_rgb(0x8b, 0x5c, 0xf6),
    track: Color32::from_rgb(0x3a, 0x3a, 0x4c),
    ok: Color32::from_rgb(0x3d, 0xd6, 0x8c),
    warn: Color32::from_rgb(0xff, 0xb4, 0x4d),
    err: Color32::from_rgb(0xff, 0x5c, 0x6c),
};

/// "Carta calda".
pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xf3, 0xed, 0xe3),
    surface: Color32::from_rgb(0xfb, 0xf8, 0xf2),
    text: Color32::from_rgb(0x2b, 0x25, 0x20),
    text_dim: Color32::from_rgb(0x85, 0x77, 0x67),
    accent: Color32::from_rgb(0xc2, 0x41, 0x0c),
    track: Color32::from_rgb(0xdd, 0xd1, 0xbd),
    ok: Color32::from_rgb(0x1d, 0x9e, 0x75),
    warn: Color32::from_rgb(0xba, 0x75, 0x17),
    err: Color32::from_rgb(0xd8, 0x5a, 0x30),
};

/// The palette of the theme in use.
pub fn palette(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}

const ROW_HEIGHT: f32 = 40.0;
const CONTROL_HEIGHT: f32 = 28.0;
const SLIDER_WIDTH: f32 = 220.0;
const TITLE_FAMILY: &str = "title";

/// Font for titles and the name of the application.
pub fn title_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(TITLE_FAMILY.into()))
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::System);
    ctx.set_visuals_of(egui::Theme::Dark, visuals(&DARK, egui::Visuals::dark()));
    ctx.set_visuals_of(egui::Theme::Light, visuals(&LIGHT, egui::Visuals::light()));
    ctx.all_styles_mut(|s| {
        use egui::TextStyle;
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 5.0);
        s.spacing.interact_size.y = CONTROL_HEIGHT;
        s.spacing.menu_margin = egui::Margin::same(4);
        s.spacing.combo_width = 160.0;
        // A solid bar keeps its own column, so it never covers the groups.
        s.spacing.scroll = egui::style::ScrollStyle {
            bar_outer_margin: 8.0,
            ..egui::style::ScrollStyle::solid()
        };
        s.text_styles.insert(TextStyle::Heading, title_font(26.0));
        s.text_styles
            .insert(TextStyle::Body, FontId::proportional(14.0));
        s.text_styles
            .insert(TextStyle::Button, FontId::proportional(14.0));
        s.text_styles
            .insert(TextStyle::Small, FontId::proportional(12.0));
        s.text_styles
            .insert(TextStyle::Monospace, FontId::monospace(13.0));
    });
    install_fonts(ctx);
}

fn visuals(p: &Palette, mut v: egui::Visuals) -> egui::Visuals {
    v.panel_fill = p.surface;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, hairline(p));
    v.window_corner_radius = CornerRadius::same(20);
    v.menu_corner_radius = CornerRadius::same(16);
    v.extreme_bg_color = p.bg;
    v.faint_bg_color = p.bg;
    v.text_edit_bg_color = Some(p.bg);
    v.override_text_color = Some(p.text);
    v.weak_text_color = Some(p.text_dim);
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.err;
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(CONTROL_HEIGHT as u8 / 2);
        w.bg_stroke = Stroke::NONE;
        w.fg_stroke.color = p.text;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, hairline(p));
    v.widgets.noninteractive.fg_stroke.color = p.text_dim;
    v.widgets.inactive.weak_bg_fill = p.track;
    v.widgets.inactive.bg_fill = p.track;
    v.widgets.hovered.weak_bg_fill = p.track.gamma_multiply(if v.dark_mode { 1.3 } else { 0.9 });
    v.widgets.hovered.bg_fill = v.widgets.hovered.weak_bg_fill;
    v.widgets.hovered.expansion = 0.0;
    v.widgets.active.weak_bg_fill = p.accent;
    v.widgets.active.bg_fill = p.accent;
    v.widgets.active.fg_stroke.color = Color32::WHITE;
    v.widgets.active.expansion = 0.0;
    v.widgets.open.weak_bg_fill = p.track;
    v.widgets.open.bg_fill = p.track;
    v
}

/// Separator between rows and the outline of popups.
fn hairline(p: &Palette) -> Color32 {
    p.text_dim.gamma_multiply(0.25)
}

/// Inter for everything, Rajdhani for titles; a system font covers the symbols neither has.
fn install_fonts(ctx: &egui::Context) {
    ctx.add_font(FontInsert::new(
        "Inter",
        FontData::from_static(include_bytes!("../../assets/fonts/Inter-Regular.ttf")),
        vec![InsertFontFamily {
            family: FontFamily::Proportional,
            priority: FontPriority::Highest,
        }],
    ));
    ctx.add_font(FontInsert::new(
        "Rajdhani",
        FontData::from_static(include_bytes!("../../assets/fonts/Rajdhani-SemiBold.ttf")),
        vec![InsertFontFamily {
            family: FontFamily::Name(TITLE_FAMILY.into()),
            priority: FontPriority::Highest,
        }],
    ));
    install_fallback_fonts(ctx);
}

/// System fonts that cover the symbols the bundled fonts lack (sidebar icons, arrows, dots).
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
                        family: FontFamily::Name(TITLE_FAMILY.into()),
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

/// A group of rows: a small title above, a rounded container, an optional explanation below.
pub fn group(ui: &mut egui::Ui, title: &str, hint: Option<&str>, add: impl FnOnce(&mut egui::Ui)) {
    let p = palette(ui);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(RichText::new(title).color(p.text_dim).size(12.5));
    });
    let width = ui.available_width();
    egui::Frame::new()
        .fill(p.bg)
        .corner_radius(CornerRadius::same(16))
        .inner_margin(egui::Margin::same(4))
        .show(ui, |ui| {
            ui.set_width(width - 8.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
    if let Some(hint) = hint {
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.add(egui::Label::new(dim(hint)).wrap());
        });
    }
    ui.add_space(12.0);
}

/// One row of a group: the label on the left, the controls added by `add` on the right
/// (laid out right to left, so the first widget added is the rightmost).
pub fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    let p = palette(ui);
    let first = ui.min_size().y == 0.0;
    let size = egui::vec2(ui.available_width(), ROW_HEIGHT);
    ui.allocate_ui_with_layout(
        size,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_size(size);
            let rect = ui.max_rect();
            if !first {
                let y = rect.top();
                ui.painter().hline(
                    (rect.left() + 12.0)..=(rect.right() - 12.0),
                    y,
                    Stroke::new(1.0, hairline(p)),
                );
            }
            ui.add_space(8.0);
            ui.label(label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                add(ui);
            });
        },
    );
}

/// Caption in the secondary text colour.
pub fn dim(text: impl Into<String>) -> RichText {
    RichText::new(text).weak().small()
}

/// The primary action of a page: the only accent-filled button.
pub fn primary(ui: &egui::Ui, text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).color(Color32::WHITE)).fill(palette(ui).accent)
}

/// The current value in a capsule that opens a menu with a check mark on the chosen option.
/// Returns the option clicked, when it differs from `selected`.
pub fn choice<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id_salt: impl egui::AsIdSalt,
    selected: T,
    options: &[(T, String)],
) -> Option<T> {
    let p = palette(ui);
    let current = options
        .iter()
        .find(|(x, _)| *x == selected)
        .map(|(_, name)| name.as_str())
        .unwrap_or("—");
    let mut chosen = None;
    ui.push_id(id_salt, |ui| {
        ui.menu_button(format!("{current}  ⌄"), |ui| {
            ui.set_min_width(170.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            for (x, name) in options {
                let on = *x == selected;
                let mark = if on { "✓" } else { " " };
                let text = RichText::new(format!("{name}   {mark}")).color(if on {
                    Color32::WHITE
                } else {
                    p.text
                });
                let item = egui::Button::new(text)
                    .fill(if on { p.accent } else { Color32::TRANSPARENT })
                    .corner_radius(CornerRadius::same(12))
                    .min_size(egui::vec2(ui.available_width(), 32.0));
                if ui.add(item).clicked() && !on {
                    chosen = Some(*x);
                }
            }
        });
    });
    chosen
}

/// Slider with the value in an editable capsule next to it. `changed()` while dragging,
/// `drag_stopped()` on release, `changed() && !dragged()` when the value is typed.
pub fn slider(ui: &mut egui::Ui, value: &mut u16, max: u16) -> Response {
    let p = palette(ui);
    let size = egui::vec2(SLIDER_WIDTH + 8.0 + 48.0, CONTROL_HEIGHT);
    ui.allocate_ui_with_layout(
        size,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let (rect, mut rail) = ui.allocate_exact_size(
                egui::vec2(SLIDER_WIDTH, CONTROL_HEIGHT),
                egui::Sense::click_and_drag(),
            );
            let knob = 8.0;
            let span = (rect.left() + knob)..=(rect.right() - knob);
            if (rail.dragged() || rail.clicked())
                && let Some(pos) = rail.interact_pointer_pos()
            {
                let f = egui::remap_clamp(pos.x, span.clone(), 0.0..=1.0);
                let new = (f * f32::from(max)).round() as u16;
                if new != *value {
                    *value = new;
                    rail.mark_changed();
                }
            }
            if ui.is_rect_visible(rect) {
                let f = f32::from(*value) / f32::from(max.max(1));
                let x = egui::lerp(span, f);
                let track =
                    egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 6.0));
                let painter = ui.painter();
                painter.rect_filled(track, 3.0, p.track);
                let filled = egui::Rect::from_min_max(track.min, egui::pos2(x, track.max.y));
                painter.rect_filled(filled, 3.0, fade_if_disabled(ui, p.accent));
                let centre = egui::pos2(x, rect.center().y);
                painter.circle(
                    centre,
                    knob,
                    fade_if_disabled(ui, Color32::WHITE),
                    Stroke::new(1.0, Color32::from_black_alpha(60)),
                );
            }
            ui.add_space(8.0);
            let number = ui.add_sized(
                [48.0, CONTROL_HEIGHT],
                egui::DragValue::new(value).range(0..=max).speed(0.2),
            );
            rail.union(number)
        },
    )
    .inner
}

/// Dims a colour when the ui is disabled.
fn fade_if_disabled(ui: &egui::Ui, color: Color32) -> Color32 {
    if ui.is_enabled() {
        color
    } else {
        color.gamma_multiply(0.5)
    }
}

/// Switch.
pub fn toggle(ui: &mut egui::Ui, on: &mut bool) -> Response {
    let p = palette(ui);
    let size = egui::vec2(44.0, 24.0);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_with_time(response.id, *on, 0.15);
        let radius = 0.5 * rect.height();
        let fill = fade_if_disabled(ui, if *on { p.accent } else { p.track });
        ui.painter()
            .rect(rect, radius, fill, Stroke::NONE, egui::StrokeKind::Inside);
        let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
        ui.painter().circle(
            egui::pos2(x, rect.center().y),
            radius - 3.0,
            fade_if_disabled(ui, Color32::WHITE),
            Stroke::new(1.0, Color32::from_black_alpha(40)),
        );
    }
    response
}
