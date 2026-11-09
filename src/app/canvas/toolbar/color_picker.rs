//! A staged color palette: preview locally, apply only when confirmed.

use super::{
    ToolbarAction,
    icons::{self, Icon},
    widgets,
};
use crate::app::theme;
use egui::{Color32, CornerRadius, Rect, Sense, Stroke, StrokeKind, vec2};

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
    hex: String,
    editing: bool,
    recent: Vec<[u8; 3]>,
}

impl Default for ColorPicker {
    fn default() -> Self {
        Self {
            pending: [0; 3],
            hex: "#000000".to_owned(),
            editing: false,
            recent: vec![
                [163, 33, 59],
                [233, 107, 0],
                [215, 215, 211],
                [104, 55, 135],
                [155, 193, 235],
            ],
        }
    }
}

impl ColorPicker {
    pub fn button(
        &mut self,
        ui: &mut egui::Ui,
        current: [u8; 3],
        action: &mut Option<ToolbarAction>,
    ) {
        let response = icons::button(ui, Icon::Palette, "选择颜色", false, true);
        if response.clicked() {
            self.choose(current);
            self.editing = false;
        }
        egui::Popup::menu(&response)
            .frame(theme::popup_frame())
            .gap(10.0)
            .width(350.0)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| self.panel(ui, action));
    }

    fn choose(&mut self, rgb: [u8; 3]) {
        self.pending = rgb;
        self.hex = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
    }

    fn panel(&mut self, ui: &mut egui::Ui, action: &mut Option<ToolbarAction>) {
        ui.set_width(350.0);
        widgets::heading(ui, "拾取颜色");
        ui.spacing_mut().item_spacing = vec2(3.0, 2.0);
        for row in 0..5 {
            ui.horizontal(|ui| {
                for column in SHADES {
                    let value = column[row];
                    let rgb = [(value >> 16) as u8, (value >> 8) as u8, value as u8];
                    let color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                    let (rect, response) = ui.allocate_exact_size(vec2(36.0, 25.0), Sense::click());
                    let radius = CornerRadius {
                        nw: if row == 0 { 7 } else { 0 },
                        ne: if row == 0 { 7 } else { 0 },
                        sw: if row == 4 { 7 } else { 0 },
                        se: if row == 4 { 7 } else { 0 },
                    };
                    ui.painter().rect_filled(rect, radius, color);
                    if rgb == self.pending {
                        ui.painter().rect_stroke(
                            rect.shrink(0.5),
                            radius,
                            Stroke::new(2.0, theme::ACCENT),
                            StrokeKind::Inside,
                        );
                        let light = u32::from(rgb[0]) * 299
                            + u32::from(rgb[1]) * 587
                            + u32::from(rgb[2]) * 114
                            > 165_000;
                        icons::paint(
                            ui.painter(),
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
                    if response.on_hover_text(format!("#{value:06X}")).clicked() {
                        self.choose(rgb);
                    }
                }
            });
        }
        widgets::section(ui, "自定义");
        ui.horizontal(|ui| {
            if icons::button(ui, Icon::Plus, "添加自定义颜色", self.editing, true).clicked()
            {
                self.editing = !self.editing;
            }
            let mut chosen = None;
            for &rgb in &self.recent {
                let (rect, response) = ui.allocate_exact_size(vec2(32.0, 30.0), Sense::click());
                ui.painter()
                    .rect_filled(rect, 5.0, Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
                if response.hovered() {
                    ui.painter().rect_stroke(
                        rect,
                        5.0,
                        Stroke::new(1.5, theme::ACCENT),
                        StrokeKind::Inside,
                    );
                }
                if response.clicked() {
                    chosen = Some(rgb);
                }
            }
            if let Some(rgb) = chosen {
                self.choose(rgb);
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let (preview, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
            ui.painter().circle_filled(
                preview.center(),
                10.0,
                Color32::from_rgb(self.pending[0], self.pending[1], self.pending[2]),
            );
            ui.painter()
                .circle_stroke(preview.center(), 10.0, Stroke::new(1.0, theme::BORDER));
            if self.editing {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.hex)
                        .desired_width(110.0)
                        .char_limit(7),
                );
                if response.changed()
                    && let Ok(value) = u32::from_str_radix(self.hex.trim_start_matches('#'), 16)
                    && self.hex.trim_start_matches('#').len() == 6
                {
                    self.pending = [(value >> 16) as u8, (value >> 8) as u8, value as u8];
                }
            } else {
                ui.label(
                    egui::RichText::new(&self.hex)
                        .size(12.0)
                        .color(theme::MUTED),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("选择").strong().color(theme::SURFACE),
                        )
                        .fill(theme::ACCENT)
                        .min_size(vec2(64.0, 32.0)),
                    )
                    .clicked()
                {
                    *action = Some(ToolbarAction::Color(self.pending));
                    self.recent.retain(|rgb| *rgb != self.pending);
                    self.recent.insert(0, self.pending);
                    self.recent.truncate(7);
                    ui.close();
                }
                if ui
                    .add(egui::Button::new("取消").min_size(vec2(58.0, 32.0)))
                    .clicked()
                {
                    ui.close();
                }
            });
        });
        ui.add_space(2.0);
    }
}
