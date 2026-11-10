//! Canvas overlays: quick colors, pen width and the tool dock.

use super::{
    CanvasTool, ToolbarAction, ToolbarContext, WorkspaceToolbar,
    icons::{self, Icon},
};
use crate::app::theme;
use egui::{Align2, Color32, Sense, Stroke, StrokeKind, vec2};

const COLORS: [([u8; 3], &str); 9] = [
    ([0, 0, 0], "黑色"),
    ([255, 255, 255], "白色"),
    ([104, 55, 135], "紫色"),
    ([155, 193, 235], "浅蓝"),
    ([32, 100, 178], "蓝色"),
    ([33, 159, 109], "绿色"),
    ([247, 196, 14], "黄色"),
    ([234, 111, 0], "橙色"),
    ([237, 51, 65], "红色"),
];

fn area(
    context: &egui::Context,
    id: &str,
    anchor: Align2,
    offset: egui::Vec2,
    show: impl FnOnce(&mut egui::Ui),
) {
    egui::Area::new(egui::Id::unique(id))
        .order(egui::Order::Foreground)
        .anchor(anchor, offset)
        .movable(false)
        .show(context, |ui| {
            theme::floating_frame().show(ui, show);
        });
}

pub(super) fn notice(
    context: &egui::Context,
    message: &str,
    bottom_inset: f32,
    action: &mut Option<ToolbarAction>,
) {
    let offset = if context.content_rect().width() < 1120.0 {
        134.0
    } else {
        76.0
    };
    area(
        context,
        "document_notice",
        Align2::LEFT_BOTTOM,
        vec2(16.0, -offset - bottom_inset),
        |ui| {
            ui.set_max_width((context.content_rect().width() - 110.0).min(460.0));
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(message).color(Color32::from_rgb(255, 180, 180)),
                    )
                    .truncate(),
                )
                .on_hover_text(message);
                if icons::button(ui, Icon::Close, "关闭提示", false, true).clicked() {
                    *action = Some(ToolbarAction::DismissNotice);
                }
            });
        },
    );
}

impl WorkspaceToolbar {
    pub(super) fn floating(
        &mut self,
        context: &egui::Context,
        state: &ToolbarContext<'_>,
        action: &mut Option<ToolbarAction>,
    ) {
        area(
            context,
            "color_palette",
            Align2::CENTER_TOP,
            vec2(0.0, 58.0),
            |ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.horizontal(|ui| {
                    for (rgb, name) in COLORS {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                        let color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                        ui.painter().rect_filled(rect.shrink(1.0), 4.0, color);
                        ui.painter().rect_stroke(
                            rect.shrink(1.0),
                            4.0,
                            Stroke::new(1.0, Color32::from_white_alpha(24)),
                            StrokeKind::Inside,
                        );
                        if state.style.color_rgb == rgb {
                            ui.painter().rect_stroke(
                                rect.expand(1.0),
                                6.0,
                                Stroke::new(1.5, theme::ACCENT),
                                StrokeKind::Outside,
                            );
                        } else if response.hovered() {
                            ui.painter().rect_stroke(
                                rect,
                                5.0,
                                Stroke::new(1.0, theme::TEXT),
                                StrokeKind::Inside,
                            );
                        }
                        if response.on_hover_text(name).clicked() {
                            *action = Some(ToolbarAction::Color(rgb));
                        }
                    }
                    let mut color = state.style.color_rgb;
                    if self.color_picker.button(ui, &mut color).changed() {
                        *action = Some(ToolbarAction::Color(color));
                    }
                });
            },
        );
        area(
            context,
            "pen_properties",
            Align2::RIGHT_CENTER,
            vec2(-18.0 - if self.settings_open { 320.0 } else { 0.0 }, 12.0),
            |ui| {
                ui.set_width(34.0);
                ui.set_height((context.content_rect().height() - 200.0).clamp(260.0, 390.0));
                ui.vertical_centered(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(32.0, 38.0), Sense::hover());
                    icons::paint(
                        ui,
                        egui::Rect::from_center_size(rect.center(), vec2(24.0, 24.0)),
                        Icon::Pen,
                        theme::TEXT,
                    );
                    super::settings::button(ui, state, action);
                    ui.add_space(4.0);
                    ui.separator();
                    egui::Frame::NONE
                        .fill(theme::CARD)
                        .corner_radius(8)
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            if icons::button(ui, Icon::Plus, "增大笔宽", false, true).clicked()
                            {
                                *action = Some(ToolbarAction::Width(
                                    (state.style.max_width_pt + 0.2).min(24.0),
                                ));
                            }
                            let mut width = state.style.max_width_pt;
                            if ui
                                .add_sized(
                                    [32.0, 26.0],
                                    egui::DragValue::new(&mut width)
                                        .speed(0.1)
                                        .range(0.2..=24.0)
                                        .fixed_decimals(1),
                                )
                                .on_hover_text("最大笔宽 · pt")
                                .changed()
                            {
                                *action = Some(ToolbarAction::Width(width));
                            }
                            if icons::button(ui, Icon::Minus, "减小笔宽", false, true).clicked()
                            {
                                *action = Some(ToolbarAction::Width(
                                    (state.style.max_width_pt - 0.2).max(0.2),
                                ));
                            }
                        });
                    ui.add_space(10.0);
                    for (width, radius) in [(1.0, 1.5), (4.6, 3.3), (9.0, 5.3)] {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(32.0, 34.0), Sense::click());
                        let selected = (state.style.max_width_pt - width).abs() < 0.05;
                        if response.hovered() || selected {
                            ui.painter().rect_filled(
                                rect,
                                7.0,
                                if selected {
                                    theme::SELECTED
                                } else {
                                    theme::HOVER
                                },
                            );
                        }
                        ui.painter()
                            .circle_filled(rect.center(), radius, theme::TEXT);
                        if response
                            .on_hover_text(format!("笔宽 {width:.1} pt"))
                            .clicked()
                        {
                            *action = Some(ToolbarAction::Width(width));
                        }
                    }
                });
            },
        );
        let bottom = if context.content_rect().width() < 1120.0 {
            72.0
        } else {
            16.0
        };
        area(
            context,
            "tool_dock",
            Align2::CENTER_BOTTOM,
            vec2(0.0, -bottom - state.bottom_inset),
            |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.horizontal(|ui| {
                    for (icon, label, tool) in [
                        (Icon::Pen, "画笔", CanvasTool::Pen),
                        (Icon::Hand, "移动工具", CanvasTool::Pan),
                    ] {
                        if icons::button(ui, icon, label, state.document.tool == tool, true)
                            .clicked()
                            && state.document.tool != tool
                        {
                            *action = Some(ToolbarAction::Tool(tool));
                        }
                    }
                    icons::separator(ui);
                    for (icon, label) in [
                        (Icon::Marker, "荧光笔"),
                        (Icon::Text, "文本"),
                        (Icon::Eraser, "橡皮擦"),
                        (Icon::Select, "选区"),
                    ] {
                        icons::button(ui, icon, label, false, false);
                    }
                    icons::separator(ui);
                    if icons::button(
                        ui,
                        Icon::Undo,
                        "撤销 · Ctrl+Z",
                        false,
                        state.document.can_undo,
                    )
                    .clicked()
                    {
                        *action = Some(ToolbarAction::Undo);
                    }
                    if icons::button(
                        ui,
                        Icon::Redo,
                        "重做 · Ctrl+Y / Ctrl+Shift+Z",
                        false,
                        state.document.can_redo,
                    )
                    .clicked()
                    {
                        *action = Some(ToolbarAction::Redo);
                    }
                });
            },
        );
    }
}
