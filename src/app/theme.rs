//! Shared colors and spacing for the editor chrome.

use egui::{Color32, CornerRadius, Stroke};

pub(crate) const WORKSPACE: Color32 = Color32::from_rgb(50, 50, 54);
pub(crate) const HEADER: Color32 = Color32::from_rgb(45, 45, 48);
pub(crate) const SURFACE: Color32 = Color32::from_rgb(38, 38, 42);
pub(crate) const CARD: Color32 = Color32::from_rgb(62, 62, 68);
pub(crate) const HOVER: Color32 = Color32::from_rgb(73, 73, 81);
pub(crate) const BORDER: Color32 = Color32::from_rgb(77, 77, 85);
pub(crate) const SELECTED: Color32 = Color32::from_rgb(58, 77, 103);
pub(crate) const TEXT: Color32 = Color32::from_rgb(238, 238, 240);
pub(crate) const MUTED: Color32 = Color32::from_rgb(168, 168, 180);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(143, 182, 239);

pub(crate) fn configure(context: &egui::Context) {
    let mut style = egui::Style {
        visuals: egui::Visuals::dark(),
        ..Default::default()
    };
    style.visuals.panel_fill = WORKSPACE;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = SURFACE;
    style.visuals.faint_bg_color = HEADER;
    style.visuals.window_stroke = Stroke::new(1.0, BORDER);
    style.visuals.window_corner_radius = CornerRadius::same(10);
    style.visuals.selection.bg_fill = Color32::from_rgb(43, 83, 110);
    style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(19.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(12.5));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(11.0));
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widget.corner_radius = CornerRadius::same(6);
        widget.bg_stroke = Stroke::NONE;
        widget.fg_stroke = Stroke::new(1.4, TEXT);
    }
    style.visuals.widgets.inactive.bg_fill = CARD;
    style.visuals.widgets.inactive.weak_bg_fill = CARD;
    style.visuals.widgets.hovered.bg_fill = HOVER;
    style.visuals.widgets.hovered.weak_bg_fill = HOVER;
    style.visuals.widgets.active.bg_fill = HOVER;
    style.spacing.item_spacing = egui::vec2(5.0, 5.0);
    style.spacing.button_padding = egui::vec2(10.0, 7.0);
    style.spacing.interact_size = egui::vec2(30.0, 30.0);
    style.spacing.scroll.bar_width = 10.0;
    context.set_theme(egui::Theme::Dark);
    context.set_style_of(egui::Theme::Dark, style);
}

pub(super) fn floating_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(12)
        .inner_margin(6)
        .shadow(egui::Shadow {
            offset: [0, 5],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha(80),
        })
}

pub(super) fn popup_frame() -> egui::Frame {
    floating_frame().inner_margin(14).corner_radius(15)
}
