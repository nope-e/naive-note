//! Grouped brush settings. Unsupported curve fitting stays an explicit placeholder.

use super::{
    ToolbarAction, ToolbarContext,
    icons::{self, Icon},
    widgets,
};
use crate::app::theme;
use egui::{
    Align2, FontId, Sense, Stroke,
    containers::menu::{MenuConfig, MenuState, SubMenu},
    vec2,
};

const CURVES: [(&str, f64); 6] = [
    ("固定值", 0.0),
    ("线性", 1.0),
    ("平方根", 0.5),
    ("立方根", 1.0 / 3.0),
    ("二次抛物线", 2.0),
    ("三次抛物线", 3.0),
];

pub(super) fn button(
    ui: &mut egui::Ui,
    state: &ToolbarContext<'_>,
    action: &mut Option<ToolbarAction>,
) {
    let response = icons::button(ui, Icon::Settings, "画笔设置", false, true);
    egui::Popup::menu(&response)
        .frame(theme::popup_frame())
        .align(egui::RectAlign::LEFT_START)
        .align_alternatives(&[egui::RectAlign::LEFT_END])
        .gap(18.0)
        .width(304.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| show(ui, state, action));
}

fn show(ui: &mut egui::Ui, state: &ToolbarContext<'_>, action: &mut Option<ToolbarAction>) {
    ui.set_width(304.0);
    widgets::heading(ui, "画笔设置");
    let height = (ui.ctx().content_rect().height() - 150.0).clamp(220.0, 600.0);
    egui::ScrollArea::vertical()
        .max_height(height)
        .show(ui, |ui| {
            widgets::section(ui, "路径模式");
            ui.spacing_mut().item_spacing.y = 3.0;
            if widgets::choice(
                ui,
                "简单",
                "按照原始输入生成线段。",
                !state.document.smoothing,
                true,
            )
            .clicked()
                && state.document.smoothing
            {
                *action = Some(ToolbarAction::Smoothing);
            }
            widgets::choice(ui, "曲线", "生成平滑的曲线段。", false, false);
            if widgets::choice(
                ui,
                "建模",
                "模拟笔尖运动，平滑笔迹并保留压力变化。",
                state.document.smoothing,
                true,
            )
            .clicked()
                && !state.document.smoothing
            {
                *action = Some(ToolbarAction::Smoothing);
            }
            if state.document.tool == super::CanvasTool::Highlighter {
                widgets::section(ui, "荧光笔样式");
                ui.label("压感笔宽 · 40% 不透明度 · 正片叠底");
            } else {
                widgets::section(ui, "硬笔样式");
            }
            pressure_curve(ui, state.style.pressure_gamma, action);
            widgets::section(ui, "输入");
            if widgets::switch(
                ui,
                "鼠标书写",
                "画笔模式下使用左键书写",
                state.document.mouse_pen,
            )
            .clicked()
            {
                *action = Some(ToolbarAction::MousePen);
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("设置从下一笔开始生效")
                        .size(11.0)
                        .color(theme::MUTED),
                );
            });
            ui.add_space(4.0);
        });
    ui.allocate_space(vec2(0.0, 2.0));
}

fn pressure_curve(ui: &mut egui::Ui, current: f64, action: &mut Option<ToolbarAction>) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let close_id = response.id.with("close_curve_menu");
    // Keep the child registered until the parent has handled this frame's click.
    // Otherwise choosing a curve is mistaken for a click outside the settings popup.
    if ui.data_mut(|data| data.remove_temp::<bool>(close_id)) == Some(true) {
        MenuState::from_ui(ui, |menu, _| menu.open_item = None);
    }
    let open = MenuState::from_ui(ui, |menu, _| {
        menu.open_item == Some(SubMenu::id_from_widget_id(response.id))
    });
    ui.painter().rect_filled(
        rect,
        10.0,
        if response.hovered() || open {
            theme::HOVER
        } else {
            theme::CARD
        },
    );
    ui.painter().text(
        rect.min + vec2(12.0, 9.0),
        Align2::LEFT_TOP,
        "压力曲线",
        FontId::proportional(13.0),
        theme::TEXT,
    );
    ui.painter().text(
        rect.min + vec2(12.0, 32.0),
        Align2::LEFT_TOP,
        "选择一种压力曲线",
        FontId::proportional(11.0),
        theme::MUTED,
    );
    let label = CURVES
        .iter()
        .find(|(_, gamma)| (*gamma - current).abs() < 1e-9)
        .map_or_else(
            || format!("自定义 ({current:.1})"),
            |(label, _)| (*label).to_owned(),
        );
    ui.painter().text(
        rect.right_center() - vec2(34.0, 0.0),
        Align2::RIGHT_CENTER,
        label,
        FontId::proportional(12.5),
        theme::TEXT,
    );
    let arrow = rect.right_center() - vec2(18.0, 0.0);
    ui.painter().add(egui::Shape::line(
        vec![
            arrow + vec2(-4.0, -2.0),
            arrow + vec2(0.0, 2.0),
            arrow + vec2(4.0, -2.0),
        ],
        Stroke::new(1.5, theme::TEXT),
    ));

    // A ComboBox uses the same global popup slot as the settings panel. A submenu
    // has its own state, so opening it does not replace its parent popup.
    SubMenu::new()
        .config(MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside))
        .show(ui, &response, |ui| {
            ui.set_min_width(148.0);
            for (label, gamma) in CURVES {
                if ui
                    .selectable_label((gamma - current).abs() < 1e-9, label)
                    .clicked()
                {
                    *action = Some(ToolbarAction::PressureGamma(gamma));
                    ui.data_mut(|data| data.insert_temp(close_id, true));
                    ui.request_repaint();
                }
            }
        });
}
