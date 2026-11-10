//! Shared, staged color picker with presets above an inline custom editor.

use super::{
    icons::{self, Icon},
    widgets,
};
use crate::app::theme;
use egui::{Color32, CornerRadius, Rect, Sense, Stroke, StrokeKind, vec2};

const PANEL_WIDTH: f32 = 280.0;

const SHADES: [[u32; 5]; 9] = [
    [0x9bc1eb, 0x64a3e1, 0x3587d9, 0x246dbe, 0x20559c],
    [0x91e9a2, 0x57d68a, 0x2dc77a, 0x23b078, 0x21966c],
    [0xfff26a, 0xf8e348, 0xf7d51d, 0xefc411, 0xdca90c],
    [0xffc479, 0xffa541, 0xff8c13, 0xe96b00, 0xc64d00],
    [0xf56658, 0xee3d37, 0xdd2429, 0xc31f32, 0xa3213b],
    [0xd78bdd, 0xba67cb, 0x9c4eb7, 0x833c9e, 0x683787],
    [0xc9aa91, 0xb58d68, 0x9d754e, 0x805b38, 0x64472e],
    [0xffffff, 0xe8e7e3, 0xd7d7d3, 0xbdbdb9, 0x979794],
    [0x85838a, 0x63616c, 0x43404f, 0x282532, 0x000000],
];

pub(super) struct ColorPicker {
    pending: [u8; 3],
    hsva: egui::ecolor::Hsva,
    hex: String,
    recent: Vec<[u8; 3]>,
}

impl Default for ColorPicker {
    fn default() -> Self {
        Self {
            pending: [0; 3],
            hsva: egui::ecolor::Hsva::from_srgb([0; 3]),
            hex: "#000000".to_owned(),
            recent: Vec::new(),
        }
    }
}

impl ColorPicker {
    pub fn button(&mut self, ui: &mut egui::Ui, current: &mut [u8; 3]) -> egui::Response {
        let mut response = icons::button(ui, Icon::Palette, "选择颜色", false, true)
            .on_hover_text(hex_color(*current));
        let marker = Rect::from_center_size(
            response.rect.center_bottom() - vec2(0.0, 4.0),
            vec2(18.0, 3.0),
        );
        ui.painter().rect_filled(marker, 1.0, rgb_color(*current));
        ui.painter().rect_stroke(
            marker,
            1.0,
            Stroke::new(0.5, theme::MUTED),
            StrokeKind::Outside,
        );
        if response.clicked() {
            self.choose(*current);
        }
        egui::Popup::menu(&response)
            .frame(theme::popup_frame().inner_margin(10))
            .gap(8.0)
            .width(PANEL_WIDTH)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                if let Some(rgb) = self.panel(ui) {
                    *current = rgb;
                    response.mark_changed();
                }
            });
        response
    }

    fn choose(&mut self, rgb: [u8; 3]) {
        self.pending = rgb;
        self.hsva = egui::ecolor::Hsva::from_srgb(rgb);
        self.hex = hex_color(rgb);
    }

    fn panel(&mut self, ui: &mut egui::Ui) -> Option<[u8; 3]> {
        ui.set_width(PANEL_WIDTH);
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        ui.spacing_mut().interact_size = vec2(26.0, 24.0);
        ui.spacing_mut().button_padding = vec2(7.0, 4.0);
        widgets::heading(ui, "拾取颜色");
        // Keep the confirmation row reachable even in a short window.
        egui::ScrollArea::vertical()
            .id_salt("color_picker_body")
            .max_height((ui.ctx().content_rect().height() - 170.0).max(120.0))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                widgets::section(ui, "预定义颜色");
                self.presets(ui);
                if !self.recent.is_empty() {
                    widgets::section(ui, "最近使用");
                    ui.horizontal(|ui| {
                        let mut chosen = None;
                        for &rgb in &self.recent {
                            if swatch(
                                ui,
                                rgb,
                                rgb == self.pending,
                                vec2(26.0, 22.0),
                                CornerRadius::same(5),
                            )
                            .clicked()
                            {
                                chosen = Some(rgb);
                            }
                        }
                        if let Some(rgb) = chosen {
                            self.choose(rgb);
                        }
                    });
                }
                ui.add_space(6.0);
                ui.separator();
                widgets::section(ui, "自定义颜色");
                ui.spacing_mut().slider_width = ui.available_width();
                if egui::color_picker::color_picker_hsva_2d(
                    ui,
                    &mut self.hsva,
                    egui::color_picker::Alpha::Opaque,
                ) {
                    // Keep HSV state, including the hue when the color is gray or black.
                    self.pending = self.hsva.to_srgb();
                    self.hex = hex_color(self.pending);
                }
                ui.horizontal(|ui| {
                    ui.label("HEX");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.hex)
                                .font(egui::TextStyle::Body)
                                .desired_width(110.0)
                                .char_limit(7),
                        )
                        .changed()
                        && let Some(rgb) = parse_hex(&self.hex)
                    {
                        self.pending = rgb;
                        self.hsva = egui::ecolor::Hsva::from_srgb(rgb);
                    }
                });
                if parse_hex(&self.hex).is_none() {
                    ui.label(
                        egui::RichText::new("请输入六位十六进制颜色，例如 #3587D9")
                            .small()
                            .color(ui.visuals().error_fg_color),
                    );
                }
            });
        ui.add_space(6.0);
        ui.separator();
        let mut chosen = None;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(
                    parse_hex(&self.hex).is_some(),
                    egui::Button::new(egui::RichText::new("选择").strong().color(theme::SURFACE))
                        .fill(theme::ACCENT)
                        .min_size(vec2(60.0, 28.0)),
                )
                .clicked()
            {
                chosen = Some(self.pending);
                self.recent.retain(|rgb| *rgb != self.pending);
                self.recent.insert(0, self.pending);
                self.recent.truncate(7);
                ui.close();
            }
            if ui
                .add(egui::Button::new("取消").min_size(vec2(54.0, 28.0)))
                .clicked()
            {
                ui.close();
            }
        });
        chosen
    }

    fn presets(&mut self, ui: &mut egui::Ui) {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = vec2(3.0, 2.0);
            let width = (ui.available_width() - 8.0 * 3.0) / 9.0;
            for row in 0..5 {
                ui.horizontal(|ui| {
                    for column in SHADES {
                        let value = column[row];
                        let rgb = [(value >> 16) as u8, (value >> 8) as u8, value as u8];
                        let radius = CornerRadius {
                            nw: if row == 0 { 7 } else { 0 },
                            ne: if row == 0 { 7 } else { 0 },
                            sw: if row == 4 { 7 } else { 0 },
                            se: if row == 4 { 7 } else { 0 },
                        };
                        if swatch(ui, rgb, rgb == self.pending, vec2(width, 20.0), radius).clicked()
                        {
                            self.choose(rgb);
                        }
                    }
                });
            }
        });
    }
}

fn rgb_color([r, g, b]: [u8; 3]) -> Color32 {
    Color32::from_rgb(r, g, b)
}

fn hex_color([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some([(value >> 16) as u8, (value >> 8) as u8, value as u8])
}

fn swatch(
    ui: &mut egui::Ui,
    rgb: [u8; 3],
    selected: bool,
    size: egui::Vec2,
    radius: CornerRadius,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::ColorWell, ui.is_enabled(), hex_color(rgb))
    });
    ui.painter().rect_filled(rect, radius, rgb_color(rgb));
    if selected {
        ui.painter().rect_stroke(
            rect.shrink(0.5),
            radius,
            Stroke::new(2.0, theme::ACCENT),
            StrokeKind::Inside,
        );
        let light =
            u32::from(rgb[0]) * 299 + u32::from(rgb[1]) * 587 + u32::from(rgb[2]) * 114 > 165_000;
        icons::paint(
            ui,
            Rect::from_center_size(rect.center(), vec2(16.0, 16.0)),
            Icon::Check,
            if light {
                theme::SURFACE
            } else {
                Color32::WHITE
            },
        );
    } else if response.hovered() {
        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, Color32::WHITE),
            StrokeKind::Inside,
        );
    }
    response.on_hover_text(hex_color(rgb))
}
