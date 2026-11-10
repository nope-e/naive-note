//! Persistent settings surface; controls do not close the panel on interaction.

use super::{
    ToolbarAction, ToolbarContext,
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
                        let mut changed = editor(ui, &mut layer.lines);
                        ui.add_space(16.0);
                        changed |= ui.checkbox(&mut layer.embed_in_pdf, "保存时将参考线写入 PDF 页面").changed();
                        ui.label(egui::RichText::new("关闭时仅在应用内叠加；保存仍会记录文档设置。").small().color(theme::MUTED));
                        if ui.button("采用全局参考线样式").clicked() { layer.lines = state.settings.reference_lines; changed = true; }
                        ui.add_space(20.0);
                        ui.separator();
                        widgets::section(ui, "自动加页");
                        changed |= ui.checkbox(&mut layer.auto_expand, "末尾提示线与自动加页").changed();
                        auto_expand_hint(ui);
                        if changed { *action = Some(ToolbarAction::DocumentReference(layer)); }
                    });
                } else {
                    widgets::section(ui, "鼠标指针");
                    let mut cursor = state.settings.cursor;
                    cursor_editor(ui, "悬停指针", &mut cursor.regular);
                    ui.add_space(10.0);
                    ui.checkbox(&mut cursor.show_drawing, "显示书写指针");
                    ui.add_enabled_ui(cursor.show_drawing, |ui| {
                        cursor_editor(ui, "书写指针", &mut cursor.drawing);
                    });
                    if cursor != state.settings.cursor { *action = Some(ToolbarAction::Cursor(cursor)); }
                    ui.label(egui::RichText::new("书写时自动切换；移动、缩放和对象工具使用各自的指针。").small().color(theme::MUTED));
                    ui.add_space(20.0);
                    ui.separator();
                    widgets::section(ui, "默认参考线");
                    ui.label(egui::RichText::new("用于新建文档；普通导入 PDF 默认关闭。保存过设置的文档沿用自身设置。").small().color(theme::MUTED));
                    ui.add_space(10.0);
                    let mut lines = state.settings.reference_lines;
                    if editor(ui, &mut lines) { *action = Some(ToolbarAction::GlobalReference(lines)); }
                    ui.add_space(20.0);
                    ui.separator();
                    widgets::section(ui, "默认自动加页");
                    let mut auto_expand = state.settings.auto_expand;
                    if ui.checkbox(&mut auto_expand, "末尾提示线与自动加页").changed() { *action = Some(ToolbarAction::GlobalAutoExpand(auto_expand)); }
                    auto_expand_hint(ui);
                    ui.label(egui::RichText::new("启用规则同参考线：新建文档采用全局默认，普通导入 PDF 默认关闭，已保存文档沿用自身设置。").small().color(theme::MUTED));
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

fn auto_expand_hint(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new(
            "末页距底边 1/16 页高处显示虚线，书写越线自动追加同尺寸页面；提示线仅在应用内显示。",
        )
        .small()
        .color(theme::MUTED),
    );
}

fn cursor_editor(ui: &mut egui::Ui, label: &str, style: &mut crate::cursor::CursorStyle) {
    use crate::cursor::{CursorShape, CursorSize};
    ui.push_id(label, |ui| {
        ui.label(label);
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("shape")
                .width(126.0)
                .selected_text(style.shape.label())
                .show_ui(ui, |ui| {
                    for (shape, label) in CursorShape::ALL {
                        ui.selectable_value(&mut style.shape, shape, label);
                    }
                });
            ui.add_enabled_ui(style.shape != CursorShape::System, |ui| {
                for (size, label) in CursorSize::ALL {
                    ui.selectable_value(&mut style.size, size, label);
                }
            });
        });
    });
}

fn editor(ui: &mut egui::Ui, lines: &mut ReferenceLines) -> bool {
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
        ui.color_edit_button_srgb(&mut lines.color_rgb);
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
