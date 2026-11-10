//! Startup/open/create chooser. The coordinator blocks canvas input while it is open.

mod settings;

use egui::{Align, Align2, FontId, Layout, RichText, Sense, Stroke, StrokeKind, pos2, vec2};

use crate::{app::theme, document::PageGeometry};

use settings::{PRESETS, PageSettings, Unit};

pub(super) enum NewDocumentAction {
    Open,
    Create(PageGeometry),
    Recover,
    Cancel,
}

pub(super) struct NewDocumentDialog {
    pub open: bool,
    settings: PageSettings,
}

pub(super) struct DialogContext<'a> {
    pub bounds: egui::Rect,
    pub can_cancel: bool,
    pub loading: bool,
    pub notice: Option<&'a str>,
    pub recovery_count: usize,
}

impl Default for NewDocumentDialog {
    fn default() -> Self {
        Self {
            open: true,
            settings: PageSettings::default(),
        }
    }
}

impl NewDocumentDialog {
    pub fn show(
        &mut self,
        context: &egui::Context,
        state: DialogContext<'_>,
    ) -> Option<NewDocumentAction> {
        if !self.open {
            return None;
        }
        let DialogContext {
            bounds: available,
            can_cancel,
            loading,
            notice,
            recovery_count,
        } = state;
        // Keep the native title-bar controls outside the dimmed document area.
        context
            .layer_painter(egui::LayerId::new(
                egui::Order::Middle,
                egui::Id::unique("new_document_backdrop"),
            ))
            .rect_filled(available, 0, egui::Color32::from_black_alpha(95));
        let width = (available.width() - 64.0).clamp(480.0, 920.0);
        let body_height = (available.height() - 230.0).clamp(100.0, 490.0);
        let mut action = None;
        egui::Window::new("打开或新建文档")
            .id(egui::Id::unique("new_document"))
            .title_bar(false)
            .resizable(false)
            .movable(false)
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(
                available.center().x - (width + 36.0) * 0.5,
                available.top() + 20.0,
            ))
            .frame(theme::popup_frame().inner_margin(18))
            .show(context, |ui| {
                ui.set_width(width);
                ui.heading(if can_cancel {
                    "新建文档"
                } else {
                    "开始一份新笔记"
                });
                ui.colored_label(
                    theme::MUTED,
                    "拖入 PDF 继续书写，或选择纸张预设创建空白文档。",
                );
                ui.add_space(14.0);
                ui.add_enabled_ui(!loading, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("new_document_body")
                        .max_height(body_height)
                        .show(ui, |ui| {
                            let right_width = 220.0;
                            let left_width = (ui.available_width() - right_width - 24.0).max(220.0);
                            ui.horizontal_top(|ui| {
                                ui.allocate_ui_with_layout(
                                    vec2(left_width, 0.0),
                                    Layout::top_down(Align::Min),
                                    |ui| {
                                        self.presets(ui, &mut action);
                                    },
                                );
                                ui.add_space(12.0);
                                egui::Frame::NONE
                                    .fill(theme::HEADER)
                                    .corner_radius(10)
                                    .inner_margin(12)
                                    .show(ui, |ui| {
                                        // Frame inherits the horizontal row's layout.
                                        // Give the sidebar its own vertical content.
                                        ui.with_layout(Layout::top_down(Align::Min), |ui| {
                                            ui.set_width(right_width - 24.0);
                                            self.configuration(ui);
                                        });
                                    });
                            });
                        });
                });
                ui.add_space(10.0);
                ui.separator();
                if let Some(notice) = notice {
                    ui.colored_label(egui::Color32::LIGHT_RED, notice);
                }
                let geometry = self.settings.geometry();
                if let Err(error) = &geometry {
                    ui.colored_label(egui::Color32::LIGHT_RED, error.to_string());
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if loading {
                        ui.spinner();
                        ui.label("正在打开 PDF…");
                    } else if recovery_count > 0
                        && ui.button(format!("恢复草稿 ({recovery_count})")).clicked()
                    {
                        action = Some(NewDocumentAction::Recover);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                !loading && geometry.is_ok(),
                                egui::Button::new(RichText::new("创建文档").color(theme::SURFACE))
                                    .fill(theme::ACCENT)
                                    .min_size(vec2(116.0, 36.0)),
                            )
                            .clicked()
                        {
                            action = geometry.ok().map(NewDocumentAction::Create);
                        }
                        if can_cancel && ui.button("取消").clicked() {
                            action = Some(NewDocumentAction::Cancel);
                        }
                    });
                });
            });
        if can_cancel && context.input(|input| input.key_pressed(egui::Key::Escape)) {
            action = Some(NewDocumentAction::Cancel);
        }
        action
    }

    fn presets(&mut self, ui: &mut egui::Ui, action: &mut Option<NewDocumentAction>) {
        let hovering = ui.input(|input| !input.raw.hovered_files.is_empty());
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 90.0), Sense::click());
        ui.painter().rect(
            rect,
            10,
            if hovering || response.hovered() {
                theme::SELECTED
            } else {
                theme::HEADER
            },
            Stroke::new(
                1.0,
                if hovering {
                    theme::ACCENT
                } else {
                    theme::BORDER
                },
            ),
            StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center() - vec2(0.0, 12.0),
            Align2::CENTER_CENTER,
            if hovering {
                "松开以打开 PDF"
            } else {
                "将 PDF 文件拖到这里"
            },
            FontId::proportional(17.0),
            theme::TEXT,
        );
        ui.painter().text(
            rect.center() + vec2(0.0, 16.0),
            Align2::CENTER_CENTER,
            "或点击选择文件 · Ctrl+O",
            FontId::proportional(12.0),
            theme::MUTED,
        );
        if response.clicked() {
            *action = Some(NewDocumentAction::Open);
        }
        ui.add_space(14.0);
        ui.label(RichText::new("纸张预设").strong());
        ui.colored_label(theme::MUTED, "选择常用尺寸，也可以在右侧自定义。");
        ui.add_space(8.0);
        let columns = (ui.available_width() / 145.0).floor().clamp(1.0, 3.0) as usize;
        let card_width = (ui.available_width() - (columns - 1) as f32 * 8.0) / columns as f32;
        egui::Grid::new("paper_presets")
            .num_columns(columns)
            .spacing(vec2(8.0, 8.0))
            .show(ui, |ui| {
                for (index, preset) in PRESETS.iter().enumerate() {
                    let selected = self.settings.selected == Some(index);
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(card_width, 104.0), Sense::click());
                    ui.painter().rect(
                        rect,
                        8,
                        if selected {
                            theme::SELECTED
                        } else if response.hovered() {
                            theme::HOVER
                        } else {
                            theme::CARD
                        },
                        Stroke::new(
                            1.0,
                            if selected {
                                theme::ACCENT
                            } else {
                                theme::BORDER
                            },
                        ),
                        StrokeKind::Inside,
                    );
                    let paper = egui::Rect::from_center_size(
                        pos2(rect.center().x, rect.top() + 29.0),
                        vec2(34.0 * (preset.width_mm / preset.height_mm) as f32, 34.0),
                    );
                    ui.painter()
                        .rect(paper, 2, theme::TEXT, Stroke::NONE, StrokeKind::Inside);
                    ui.painter().text(
                        pos2(rect.center().x, rect.top() + 62.0),
                        Align2::CENTER_CENTER,
                        preset.name,
                        FontId::proportional(15.0),
                        theme::TEXT,
                    );
                    ui.painter().text(
                        pos2(rect.center().x, rect.top() + 85.0),
                        Align2::CENTER_CENTER,
                        format!("{} × {} mm", preset.width_mm, preset.height_mm),
                        FontId::proportional(11.0),
                        theme::MUTED,
                    );
                    if response.clicked() {
                        self.settings.select(index);
                    }
                    if (index + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
    }

    fn configuration(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("文档设置").strong().size(15.0));
        ui.colored_label(
            theme::MUTED,
            self.settings
                .selected
                .map_or("自定义尺寸", |i| PRESETS[i].name),
        );
        ui.add_space(12.0);
        ui.label("尺寸单位");
        let mut unit = self.settings.unit;
        egui::ComboBox::from_id_salt("page_size_unit")
            .selected_text(unit.label())
            .width(160.0)
            .show_ui(ui, |ui| {
                for value in [Unit::Millimeters, Unit::Inches, Unit::Pixels] {
                    ui.selectable_value(&mut unit, value, value.label());
                }
            });
        if unit != self.settings.unit {
            self.settings.set_unit(unit);
        }
        ui.add_space(8.0);
        for (label, value) in [
            ("宽度", &mut self.settings.width),
            ("高度", &mut self.settings.height),
        ] {
            ui.label(label);
            if ui
                .add_sized(
                    [ui.available_width(), 30.0],
                    egui::DragValue::new(value).speed(0.1).max_decimals(2),
                )
                .changed()
            {
                self.settings.selected = None;
            }
        }
        ui.add_space(8.0);
        ui.label("方向");
        let landscape = self.settings.width > self.settings.height;
        ui.horizontal(|ui| {
            let portrait_clicked = ui.selectable_label(!landscape, "纵向").clicked();
            let landscape_clicked = ui.selectable_label(landscape, "横向").clicked();
            if portrait_clicked && landscape || landscape_clicked && !landscape {
                std::mem::swap(&mut self.settings.width, &mut self.settings.height);
            }
        });
        ui.add_space(8.0);
        ui.label("分辨率 · DPI");
        if ui
            .add_sized(
                [ui.available_width(), 30.0],
                egui::DragValue::new(&mut self.settings.dpi)
                    .range(36..=2400)
                    .speed(1),
            )
            .changed()
            && self.settings.unit == Unit::Pixels
        {
            self.settings.selected = None;
        }
        ui.add_space(12.0);
        ui.separator();
        let (width, height) = self.settings.dimensions_mm();
        let (pixels_x, pixels_y) = self.settings.dimensions_px();
        ui.label(format!("{width:.2} × {height:.2} mm"));
        ui.colored_label(theme::ACCENT, format!("{pixels_x:.0} × {pixels_y:.0} px"));
        ui.add_space(6.0);
        ui.small("PDF 使用矢量纸张。DPI 用于像素尺寸换算，不改变屏幕缩放。");
    }
}
