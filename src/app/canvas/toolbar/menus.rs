//! Aligned command groups; secondary view options live under the zoom readout.

use super::{ToolbarAction, ToolbarContext, ViewAction, icons::Icon, widgets};
use crate::app::theme;

fn command(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    shortcut: &str,
    enabled: bool,
    value: ToolbarAction,
    action: &mut Option<ToolbarAction>,
) {
    if widgets::menu_row(ui, icon, label, shortcut, enabled).clicked() {
        *action = Some(value);
        ui.close();
    }
}

pub(super) fn main_menu(
    ui: &mut egui::Ui,
    context: &ToolbarContext<'_>,
    action: &mut Option<ToolbarAction>,
) {
    ui.set_width(248.0);
    ui.spacing_mut().item_spacing.y = 2.0;
    let height = (ui.ctx().content_rect().height() - 170.0).clamp(200.0, 600.0);
    egui::ScrollArea::vertical()
        .max_height(height)
        .show(ui, |ui| {
            widgets::section(ui, "文档");
            widgets::menu_row(ui, Icon::Pages, "新建文档", "Ctrl+T", false);
            command(
                ui,
                Icon::Open,
                "打开文档",
                "Ctrl+O",
                true,
                ToolbarAction::Open,
                action,
            );
            widgets::menu_row(ui, Icon::Download, "保存文档", "Ctrl+S", false);
            widgets::menu_row(ui, Icon::Download, "另存为", "Ctrl+Shift+S", false);
            command(
                ui,
                Icon::Close,
                "关闭文档",
                "Ctrl+W",
                context.document.can_close,
                ToolbarAction::Close,
                action,
            );
            ui.add_space(5.0);
            ui.separator();
            widgets::menu_row(ui, Icon::Download, "导出…", "", false);
            widgets::menu_row(ui, Icon::Pages, "打印文档", "Ctrl+P", false);
            ui.add_space(5.0);
            ui.separator();
            ui.menu_button("开发工具", |ui| {
                ui.set_width(240.0);
                command(
                    ui,
                    Icon::Pen,
                    "模拟压感笔迹",
                    "",
                    context.document.can_demo,
                    ToolbarAction::DemoStroke,
                    action,
                );
                command(
                    ui,
                    Icon::Download,
                    "导出模拟笔迹测试 PDF",
                    "",
                    context.document.can_export_demo,
                    ToolbarAction::ExportDemo,
                    action,
                );
            });
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("F12  渲染与坐标信息")
                    .small()
                    .color(theme::MUTED),
            );
        });
}

pub(super) fn view_menu(
    ui: &mut egui::Ui,
    context: &ToolbarContext<'_>,
    action: &mut Option<ToolbarAction>,
) {
    ui.set_width(216.0);
    widgets::section(ui, "缩放");
    for (label, shortcut, value) in [
        ("适合整页", "Ctrl+0", ViewAction::Fit),
        ("适合宽度", "Ctrl+2", ViewAction::FitWidth),
        ("实际大小", "Ctrl+1", ViewAction::ActualSize),
    ] {
        if widgets::menu_row(ui, Icon::None, label, shortcut, true).clicked() {
            *action = Some(ToolbarAction::View(value));
        }
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("自定义").color(theme::MUTED));
        let mut percent = context.zoom * 100.0;
        if ui
            .add(
                egui::DragValue::new(&mut percent)
                    .range(10.0..=800.0)
                    .suffix("%")
                    .update_while_editing(false),
            )
            .changed()
        {
            *action = Some(ToolbarAction::View(ViewAction::Zoom(
                percent / 100.0 / context.zoom,
            )));
        }
    });
    ui.separator();
    if widgets::menu_row(ui, Icon::None, "顺时针旋转 90°", "", true).clicked() {
        *action = Some(ToolbarAction::View(ViewAction::Rotate));
    }
}
