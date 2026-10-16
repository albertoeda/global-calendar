//! Fonts and egui visuals for the terminal look.

use crate::theme::Palette;
use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke,
    TextStyle,
};
use std::sync::Arc;

pub const LABEL_FONT: f32 = 13.0;
pub const SMALL_FONT: f32 = 11.0;

const REGULAR: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf");

/// JetBrains Mono everywhere, with egui's default fonts kept as glyph fallbacks.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("jbm".into(), Arc::new(FontData::from_static(REGULAR)));
    fonts
        .font_data
        .insert("jbm-bold".into(), Arc::new(FontData::from_static(BOLD)));
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "jbm".into());
    }
    let mut bold = vec!["jbm-bold".to_string()];
    bold.extend(fonts.families[&FontFamily::Monospace].iter().cloned());
    fonts.families.insert(bold_family(), bold);
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::monospace(SMALL_FONT)),
            (TextStyle::Body, FontId::monospace(14.0)),
            (TextStyle::Button, FontId::monospace(14.0)),
            (TextStyle::Heading, FontId::new(16.0, bold_family())),
            (TextStyle::Monospace, FontId::monospace(14.0)),
        ]
        .into();
        style.spacing.button_padding = egui::vec2(6.0, 2.0);
        style.spacing.item_spacing = egui::vec2(8.0, 4.0);
    });
}

pub fn bold_family() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, bold_family())
}

/// Square corners, palette colours, accent on hover/focus.
pub fn apply(ctx: &egui::Context, pal: &Palette) {
    let mut v = if pal.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = pal.panel;
    v.window_fill = pal.panel;
    v.faint_bg_color = pal.panel;
    v.extreme_bg_color = pal.bg;
    v.override_text_color = Some(pal.text);
    v.window_stroke = Stroke::new(1.0, pal.border);
    v.window_corner_radius = CornerRadius::ZERO;
    v.menu_corner_radius = CornerRadius::ZERO;
    v.window_shadow = egui::Shadow::NONE;
    v.popup_shadow = egui::Shadow::NONE;
    v.selection.bg_fill = pal.accent;
    v.selection.stroke = Stroke::new(1.0, pal.bg);
    v.hyperlink_color = pal.accent;

    let ws = &mut v.widgets;
    for w in [
        &mut ws.noninteractive,
        &mut ws.inactive,
        &mut ws.hovered,
        &mut ws.active,
        &mut ws.open,
    ] {
        w.corner_radius = CornerRadius::ZERO;
        w.expansion = 0.0;
        w.bg_fill = pal.panel;
        w.weak_bg_fill = pal.panel;
        w.fg_stroke = Stroke::new(1.0, pal.text);
        w.bg_stroke = Stroke::new(1.0, pal.border);
    }
    ws.noninteractive.bg_stroke = Stroke::new(1.0, pal.border);
    ws.hovered.bg_stroke = Stroke::new(1.0, pal.accent);
    ws.hovered.fg_stroke = Stroke::new(1.0, pal.accent);
    ws.active.bg_stroke = Stroke::new(1.5, pal.accent);
    ws.active.fg_stroke = Stroke::new(1.0, pal.accent);

    ctx.set_visuals_of(egui::Theme::Dark, v.clone());
    ctx.set_visuals_of(egui::Theme::Light, v);
}

/// Frame for the status bar and the timeline bar.
pub fn bar_frame(pal: &Palette) -> egui::Frame {
    egui::Frame::NONE
        .fill(pal.panel)
        .inner_margin(Margin::symmetric(10, 6))
        .stroke(Stroke::new(1.0, pal.border))
}

pub fn status_color(pal: &Palette, status: crate::timecalc::Status) -> Color32 {
    use crate::timecalc::Status;
    match status {
        Status::Work => pal.ok,
        Status::Edge => pal.warn,
        Status::Night => pal.bad,
    }
}
