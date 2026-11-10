//! Persistent settings surface; controls do not close the panel on interaction.

use super::{
    ToolbarAction, ToolbarContext,
    color_picker::ColorPicker,
    icons::{self, Icon},
    widgets,
};
use crate::{
    app::theme,
    document::{ReferenceLines, ReferencePattern},
};

pub(super) fn show(
    ui: &mut egui::Ui,
    open: &mut bool,
    document_tab: &mut bool,
    color_picker: &mut ColorPicker,
    state: &ToolbarContext<'_>,
    action: &mut Option<ToolbarAction>,
) {
    if !*open {
        return;
    }
    egui::Panel::right("global_settings")
        .exact_size(320.0)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(theme::HEADER).inner_margin(18))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("设置");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::button(ui, Icon::Close, "关闭设置", false, true).clicked() { *open = false; }
                });
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.selectable_value(document_tab, false, "全局设置");
                ui.selectable_value(document_tab, true, "当前文档");
            });
            ui.add_space(12.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                if *document_tab {
                    widgets::section(ui, "文档参考线");
                    ui.label(egui::RichText::new(if state.reference_uses_global { "当前按全局默认显示，保存后记入文档。" } else { "当前按文档设置显示，不受全局默认影响。" }).small().color(theme::MUTED));
                    ui.add_space(10.0);
                    ui.add_enabled_ui(state.can_edit_document, |ui| {
                        let mut layer = state.reference_layer;
                        let mut changed = ui.push_id("document_reference", |ui| editor(ui, &mut layer.lines, color_picker)).inner;
                        ui.add_space(16.0);
                        changed |= ui.checkbox(&mut layer.embed_in_pdf, "保存时将参考线写入 PDF 页面").changed();
                        ui.label(egui::RichText::new("关闭时仅在应用内叠加；保存仍会记录文档设置。").small().color(theme::MUTED));
                        if ui.button("采用全局参考线样式").clicked() { layer.lines = state.settings.reference_lines; changed = true; }
                        if changed { *action = Some(ToolbarAction::DocumentReference(layer)); }
                    });
                } else {
                    widgets::section(ui, "默认参考线");
                    ui.label(egui::RichText::new("仅用于应用内新建文档。打开 PDF 时使用文档自己的设置，没有配置则不显示参考线。").small().color(theme::MUTED));
                    ui.add_space(10.0);
                    let mut lines = state.settings.reference_lines;
                    if ui.push_id("global_reference", |ui| editor(ui, &mut lines, color_picker)).inner { *action = Some(ToolbarAction::GlobalReference(lines)); }
                    ui.add_space(20.0);
                    ui.separator();
                    widgets::section(ui, "自动保存");
                    let mut seconds = state.settings.autosave_seconds;
                    ui.horizontal(|ui| {
                        ui.label("检查间隔");
                        if ui.add_enabled(seconds > 5, egui::Button::new("−")).clicked() { seconds -= 1; }
                        ui.add(egui::DragValue::new(&mut seconds).range(5..=15).suffix(" 秒"));
                        if ui.add_enabled(seconds < 15, egui::Button::new("＋")).clicked() { seconds += 1; }
                    });
                    if seconds != state.settings.autosave_seconds { *action = Some(ToolbarAction::AutosaveInterval(seconds)); }
                    ui.label(egui::RichText::new("有变化时保存；未命名草稿保留本地副本。").small().color(theme::MUTED));
                }
            });
        });
}

fn editor(ui: &mut egui::Ui, lines: &mut ReferenceLines, color_picker: &mut ColorPicker) -> bool {
    let before = *lines;
    ui.label("纹理");
    ui.horizontal_wrapped(|ui| {
        for (pattern, label) in ReferencePattern::ALL {
            ui.selectable_value(&mut lines.pattern, pattern, label);
        }
    });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.label("纹理颜色");
        color_picker.button(ui, &mut lines.color_rgb);
    });
    ui.add_space(12.0);
    for (label, value) in [
        ("纹理宽度", &mut lines.width),
        ("纹理高度", &mut lines.height),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            if ui
                .add_enabled(*value > 4.0, egui::Button::new("−"))
                .clicked()
            {
                *value = (*value - 1.0).max(4.0);
            }
            ui.add(
                egui::DragValue::new(&mut *value)
                    .range(4.0..=256.0)
                    .speed(1.0)
                    .suffix(" pt"),
            );
            if ui
                .add_enabled(*value < 256.0, egui::Button::new("＋"))
                .clicked()
            {
                *value = (*value + 1.0).min(256.0);
            }
        });
    }
    before != *lines
}
