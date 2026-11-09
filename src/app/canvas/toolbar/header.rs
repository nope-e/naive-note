//! Compact custom title bar with native window controls.

use egui::{
    Align, Align2, FontId, Layout, Rect, Sense, Stroke, UiBuilder, ViewportCommand, pos2, vec2,
};

use crate::app::theme;

use super::{
    ToolbarAction, ToolbarContext, WorkspaceToolbar,
    icons::{self, Icon},
};

const WINDOW_CONTROLS_WIDTH: f32 = 156.0;

impl WorkspaceToolbar {
    pub(super) fn header(
        &mut self,
        ui: &mut egui::Ui,
        context: &ToolbarContext<'_>,
        action: &mut Option<ToolbarAction>,
        process_input: bool,
    ) {
        egui::Panel::top("title_bar")
            .exact_size(42.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(theme::HEADER))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let drag_rect = Rect::from_min_max(
                    pos2(rect.left() + 88.0, rect.top()),
                    pos2(rect.right() - WINDOW_CONTROLS_WIDTH, rect.bottom()),
                );
                let drag = ui.interact(
                    drag_rect,
                    ui.scope_id().with("window_drag"),
                    Sense::click_and_drag(),
                );
                if process_input {
                    if drag.double_clicked() {
                        let maximized =
                            ui.input(|input| input.viewport().maximized.unwrap_or(false));
                        ui.ctx()
                            .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                    } else if drag.drag_started() {
                        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                }
                ui.scope_builder(
                    UiBuilder::new().max_rect(Rect::from_min_max(
                        rect.min + vec2(8.0, 5.0),
                        pos2(rect.left() + 86.0, rect.bottom() - 5.0),
                    )),
                    |ui| {
                        ui.horizontal(|ui| {
                            if icons::button(ui, Icon::Pages, "页面列表", self.pages_open, true)
                                .clicked()
                                && process_input
                            {
                                self.pages_open = !self.pages_open;
                                self.focus_mode = false;
                            }
                            if icons::button(ui, Icon::Eye, "专注模式", self.focus_mode, true)
                                .clicked()
                                && process_input
                            {
                                self.focus_mode = !self.focus_mode;
                            }
                        });
                    },
                );
                let title_rect = Rect::from_center_size(
                    rect.center(),
                    vec2(
                        (rect.width() - 2.0 * (WINDOW_CONTROLS_WIDTH + 12.0)).max(96.0),
                        38.0,
                    ),
                );
                let mut title_job = egui::text::LayoutJob::simple_singleline(
                    context.title.to_owned(),
                    FontId::proportional(13.0),
                    theme::TEXT,
                );
                title_job.wrap.max_width = title_rect.width();
                title_job.wrap.max_rows = 1;
                title_job.wrap.break_anywhere = true;
                let title = ui.painter().layout_job(title_job);
                ui.painter().with_clip_rect(title_rect).galley(
                    pos2(
                        title_rect.center().x - title.size().x * 0.5,
                        rect.top() + 5.0,
                    ),
                    title,
                    theme::TEXT,
                );
                ui.painter().text(
                    pos2(rect.center().x, rect.top() + 29.0),
                    Align2::CENTER_CENTER,
                    context.subtitle,
                    FontId::proportional(10.0),
                    theme::MUTED,
                );
                let right = Rect::from_min_max(
                    pos2(rect.right() - WINDOW_CONTROLS_WIDTH, rect.top() + 5.0),
                    rect.max - vec2(8.0, 5.0),
                );
                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(right)
                        .layout(Layout::right_to_left(Align::Center)),
                    |ui| {
                        for (icon, label, command) in [
                            (Icon::Close, "关闭窗口", ViewportCommand::Close),
                            (
                                Icon::Maximize,
                                "最大化 / 还原",
                                ViewportCommand::Maximized(
                                    !ui.input(|input| input.viewport().maximized.unwrap_or(false)),
                                ),
                            ),
                            (Icon::Minimize, "最小化", ViewportCommand::Minimized(true)),
                        ] {
                            let (button, response) =
                                ui.allocate_exact_size(vec2(24.0, 28.0), Sense::click());
                            let color = if response.hovered() && matches!(icon, Icon::Close) {
                                egui::Color32::from_rgb(180, 65, 72)
                            } else if response.hovered() {
                                theme::BORDER
                            } else {
                                theme::HOVER
                            };
                            ui.painter().circle_filled(button.center(), 10.0, color);
                            icons::paint(
                                ui.painter(),
                                Rect::from_center_size(button.center(), vec2(12.0, 12.0)),
                                icon,
                                theme::TEXT,
                            );
                            if response.on_hover_text(label).clicked() && process_input {
                                ui.ctx().send_viewport_cmd(command);
                            }
                        }
                        ui.add_space(5.0);
                        let menu = icons::button(ui, Icon::Menu, "菜单", false, true);
                        egui::Popup::menu(&menu)
                            .frame(theme::popup_frame())
                            .gap(10.0)
                            .width(248.0)
                            .show(|ui| super::menus::main_menu(ui, context, action));
                    },
                );
                ui.painter().line_segment(
                    [rect.left_bottom(), rect.right_bottom()],
                    Stroke::new(1.0, egui::Color32::from_black_alpha(40)),
                );
            });
    }
}
