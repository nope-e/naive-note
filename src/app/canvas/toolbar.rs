//! Editor chrome and shortcuts. Actions are applied by the workspace coordinator.

mod color_picker;
mod floating;
mod global_settings;
mod header;
mod icons;
mod menus;
mod navigation;
mod settings;
mod widgets;

use crate::{
    app::{frame_scheduler::MAX_FPS, theme},
    document::{CanvasViewport, PageGeometry, PenStyle, Point},
};

use super::{ink_input::CanvasTool, view::ViewAction};

pub(super) enum ToolbarAction {
    Open,
    New,
    Save,
    SaveAs,
    Close,
    Undo,
    Redo,
    Tool(CanvasTool),
    MousePen,
    Smoothing,
    DismissNotice,
    AutosaveInterval(u64),
    RecoverDrafts,
    Color([u8; 3]),
    Width(f64),
    PressureGamma(f64),
    GlobalReference(crate::document::ReferenceLines),
    DocumentReference(crate::document::ReferenceLayer),
    View(ViewAction),
}

pub(super) struct DocumentActions {
    pub can_close: bool,
    pub can_save: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub current_page: u32,
    pub page_count: u32,
    pub tool: CanvasTool,
    pub mouse_pen: bool,
    pub smoothing: bool,
    pub recovery_count: usize,
}

pub(super) struct ToolbarContext<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub notice: Option<&'a str>,
    pub zoom: f64,
    pub bottom_inset: f32,
    pub style: &'a PenStyle,
    pub document: DocumentActions,
    pub settings: &'a crate::storage::preferences::Settings,
    pub reference_layer: crate::document::ReferenceLayer,
    pub reference_uses_global: bool,
    pub can_edit_document: bool,
}

#[derive(Default)]
pub(super) struct WorkspaceToolbar {
    pages_open: bool,
    focus_mode: bool,
    color_picker: color_picker::ColorPicker,
    reference_color_picker: color_picker::ColorPicker,
    settings_open: bool,
    settings_document_tab: bool,
}

impl WorkspaceToolbar {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        context: &ToolbarContext<'_>,
        process_input: bool,
    ) -> Option<ToolbarAction> {
        let mut action = process_input
            .then(|| shortcuts(ui, &context.document))
            .flatten();
        self.header(ui, context, &mut action, process_input);
        global_settings::show(
            ui,
            &mut self.settings_open,
            &mut self.settings_document_tab,
            &mut self.reference_color_picker,
            context,
            &mut action,
        );
        if !self.focus_mode {
            self.sidebar(ui, context, &mut action);
            self.floating(ui.ctx(), context, &mut action);
        }
        navigation::show(ui.ctx(), context, &mut action);
        if let Some(message) = context.notice {
            floating::notice(ui.ctx(), message, context.bottom_inset, &mut action);
        }
        crate::app::window_frame::show(ui.ctx(), process_input);
        action
    }

    fn sidebar(
        &self,
        ui: &mut egui::Ui,
        context: &ToolbarContext<'_>,
        action: &mut Option<ToolbarAction>,
    ) {
        if !self.pages_open {
            return;
        }
        egui::Panel::left("pages")
            .exact_size(156.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(theme::HEADER).inner_margin(14))
            .show(ui, |ui| {
                ui.add_space(7.0);
                ui.label(egui::RichText::new("页面").strong());
                ui.add_space(12.0);
                egui::ScrollArea::vertical().show_rows(
                    ui,
                    42.0,
                    context.document.page_count as usize,
                    |ui, rows| {
                        for page in rows.map(|page| page as u32) {
                            let current = page == context.document.current_page;
                            if ui
                                .add_sized(
                                    [128.0, 38.0],
                                    egui::Button::new(format!("第 {} 页", page + 1))
                                        .selected(current),
                                )
                                .clicked()
                            {
                                *action = Some(ToolbarAction::View(ViewAction::GoToPage(page)));
                            }
                            ui.add_space(4.0);
                        }
                    },
                );
            });
    }
}

fn shortcuts(ui: &egui::Ui, document: &DocumentActions) -> Option<ToolbarAction> {
    let editing_text = ui.ctx().text_edit_focused();
    ui.input(|input| {
        if !editing_text
            && input.modifiers.ctrl
            && !input.modifiers.alt
            && ((input.modifiers.shift && input.key_pressed(egui::Key::Z))
                || input.key_pressed(egui::Key::Y))
        {
            document.can_redo.then_some(ToolbarAction::Redo)
        } else if !editing_text
            && input.modifiers.ctrl
            && !input.modifiers.alt
            && !input.modifiers.shift
            && input.key_pressed(egui::Key::Z)
        {
            document.can_undo.then_some(ToolbarAction::Undo)
        } else if input.modifiers.ctrl && !input.modifiers.alt && input.key_pressed(egui::Key::S) {
            document.can_save.then_some(if input.modifiers.shift {
                ToolbarAction::SaveAs
            } else {
                ToolbarAction::Save
            })
        } else if input.modifiers.ctrl && input.key_pressed(egui::Key::T) {
            Some(ToolbarAction::New)
        } else if input.modifiers.ctrl && input.key_pressed(egui::Key::O) {
            Some(ToolbarAction::Open)
        } else if input.modifiers.ctrl && input.key_pressed(egui::Key::W) {
            Some(ToolbarAction::Close)
        } else if input.modifiers.ctrl && input.key_pressed(egui::Key::Num0) {
            Some(ToolbarAction::View(ViewAction::Fit))
        } else if input.modifiers.ctrl && input.key_pressed(egui::Key::Num1) {
            Some(ToolbarAction::View(ViewAction::ActualSize))
        } else if input.modifiers.ctrl && input.key_pressed(egui::Key::Num2) {
            Some(ToolbarAction::View(ViewAction::FitWidth))
        } else if !editing_text && input.key_pressed(egui::Key::PageUp) {
            Some(ToolbarAction::View(ViewAction::PreviousPage))
        } else if !editing_text && input.key_pressed(egui::Key::PageDown) {
            Some(ToolbarAction::View(ViewAction::NextPage))
        } else if !editing_text && input.key_pressed(egui::Key::Home) {
            Some(ToolbarAction::View(ViewAction::GoToPage(0)))
        } else if !editing_text && input.key_pressed(egui::Key::End) {
            Some(ToolbarAction::View(ViewAction::GoToPage(
                document.page_count.saturating_sub(1),
            )))
        } else {
            None
        }
    })
}

pub(super) fn renderer_info(
    ui: &mut egui::Ui,
    page: PageGeometry,
    viewport: Option<CanvasViewport>,
    fps: Option<f64>,
) {
    let crop = page.crop_box();
    let fps = fps.map_or_else(|| "--".to_owned(), |fps| format!("{fps:.1}"));
    egui::Panel::bottom("renderer_info")
        .resizable(false)
        .frame(egui::Frame::NONE.fill(theme::HEADER).inner_margin(8))
        .show(ui, |ui| {
            ui.label(format!(
                "Skia + egui | GL | FPS: {fps} / {MAX_FPS} max | CropBox [{:.0}, {:.0}, {:.0}, {:.0}] | UserUnit {} | Rotation {} deg | F12: hide",
                crop.min.x, crop.min.y, crop.max.x, crop.max.y,
                page.user_unit(), page.rotation().degrees(),
            ));
            if let Some(viewport) = viewport {
                let bounds = viewport.physical;
                let logical_size = viewport.physical_to_logical(Point::new(f64::from(bounds.right - bounds.left), f64::from(bounds.bottom - bounds.top)));
                ui.label(format!(
                    "Canvas: {} x {} physical px | {:.1} x {:.1} logical points | {:.2} px/logical point",
                    bounds.right - bounds.left, bounds.bottom - bounds.top,
                    logical_size.x, logical_size.y, viewport.pixels_per_point,
                ));
            }
        });
}
