//! Persistent bottom-left page and zoom controls.

use super::{
    ToolbarAction, ToolbarContext, ViewAction,
    icons::{self, Icon},
};
use crate::app::theme;
use egui::{Align2, vec2};

pub(super) fn show(
    context: &egui::Context,
    state: &ToolbarContext<'_>,
    action: &mut Option<ToolbarAction>,
) {
    egui::Area::new(egui::Id::unique("document_navigation"))
        .order(egui::Order::Foreground)
        .anchor(Align2::LEFT_BOTTOM, vec2(16.0, -16.0 - state.bottom_inset))
        .movable(false)
        .show(context, |ui| {
            theme::floating_frame().show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.horizontal(|ui| {
                    if icons::button(ui, Icon::Minus, "缩小 · Ctrl+-", false, state.zoom > 0.1001)
                        .clicked()
                    {
                        *action = Some(ToolbarAction::View(ViewAction::Zoom(1.0 / 1.2)));
                    }
                    let zoom = ui.add_sized(
                        [64.0, 32.0],
                        egui::Button::new(
                            egui::RichText::new(format!("{:.0}%", state.zoom * 100.0)).size(12.0),
                        )
                        .frame(false),
                    );
                    egui::Popup::menu(&zoom)
                        .frame(theme::popup_frame())
                        .gap(10.0)
                        .width(216.0)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| super::menus::view_menu(ui, state, action));
                    if icons::button(ui, Icon::Plus, "放大 · Ctrl+=", false, state.zoom < 7.999)
                        .clicked()
                    {
                        *action = Some(ToolbarAction::View(ViewAction::Zoom(1.2)));
                    }
                    icons::separator(ui);
                    if icons::button(
                        ui,
                        Icon::Previous,
                        "上一页 · PageUp",
                        false,
                        state.document.current_page > 0,
                    )
                    .clicked()
                    {
                        *action = Some(ToolbarAction::View(ViewAction::PreviousPage));
                    }
                    let mut page = state.document.current_page + 1;
                    if ui
                        .add_sized(
                            [72.0, 32.0],
                            egui::DragValue::new(&mut page)
                                .range(1..=state.document.page_count)
                                .update_while_editing(false)
                                .suffix(format!(" / {}", state.document.page_count)),
                        )
                        .on_hover_text("点击输入页码，回车跳转")
                        .changed()
                    {
                        *action = Some(ToolbarAction::View(ViewAction::GoToPage(page - 1)));
                    }
                    if icons::button(
                        ui,
                        Icon::Next,
                        "下一页 · PageDown",
                        false,
                        state.document.current_page + 1 < state.document.page_count,
                    )
                    .clicked()
                    {
                        *action = Some(ToolbarAction::View(ViewAction::NextPage));
                    }
                });
            });
        });
}
