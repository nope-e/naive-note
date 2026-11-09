//! Toolbar widgets and shortcuts produce document or view actions.

use crate::{
    app::frame_scheduler::MAX_FPS,
    document::{CanvasViewport, PageGeometry, Point},
};

use super::view::ViewAction;

pub(super) enum ToolbarAction {
    Open,
    Close,
    DemoStroke,
    ExportDemo,
    MousePen,
    Smoothing,
    View(ViewAction),
}

pub(super) struct DocumentActions {
    pub can_close: bool,
    pub can_demo: bool,
    pub can_export_demo: bool,
    pub current_page: u32,
    pub page_count: u32,
    pub mouse_pen: bool,
    pub smoothing: bool,
}

pub(super) fn show(
    ui: &mut egui::Ui,
    zoom: f64,
    page: PageGeometry,
    viewport: Option<CanvasViewport>,
    show_renderer_info: bool,
    fps: Option<f64>,
    document: DocumentActions,
) -> Option<ToolbarAction> {
    let editing_text = ui.ctx().text_edit_focused();
    let mut action = ui.input(|input| {
        if input.modifiers.ctrl && input.key_pressed(egui::Key::O) {
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
                document.page_count - 1,
            )))
        } else {
            None
        }
    });

    egui::Panel::top("toolbar").resizable(false).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.strong("Naive Note");
            ui.separator();
            if ui.button("Open PDF").on_hover_text("Ctrl+O").clicked() {
                action = Some(ToolbarAction::Open);
            }
            if ui
                .add_enabled(document.can_close, egui::Button::new("Close PDF"))
                .on_hover_text("Ctrl+W")
                .clicked()
            {
                action = Some(ToolbarAction::Close);
            }
            ui.separator();
            if ui.selectable_label(document.mouse_pen, "Mouse pen")
                .on_hover_text("Fixed-pressure mouse drawing; turn off for left-drag pan. Session ink is not saved yet.")
                .clicked()
            {
                action = Some(ToolbarAction::MousePen);
            }
            if ui.selectable_label(document.smoothing, "Smoothing")
                .on_hover_text("Smooth pen and mouse strokes. Changes apply to the next stroke.")
                .clicked()
            {
                action = Some(ToolbarAction::Smoothing);
            }
            ui.separator();
            if ui
                .add_enabled(document.can_demo, egui::Button::new("Pressure demo"))
                .on_hover_text("Toggle one simulated pressure stroke")
                .clicked()
            {
                action = Some(ToolbarAction::DemoStroke);
            }
            if ui
                .add_enabled(
                    document.can_export_demo,
                    egui::Button::new("Export test PDF"),
                )
                .on_hover_text(
                    "Export only the simulated demo, then reopen the copy. Unavailable while unsaved session ink exists.",
                )
                .clicked()
            {
                action = Some(ToolbarAction::ExportDemo);
            }
            ui.separator();
            if ui
                .add_enabled(document.current_page > 0, egui::Button::new("Prev"))
                .on_hover_text("Page Up")
                .clicked()
            {
                action = Some(ToolbarAction::View(ViewAction::PreviousPage));
            }
            let mut number = document.current_page + 1;
            if ui
                .add(
                    egui::DragValue::new(&mut number)
                        .range(1..=document.page_count)
                        .update_while_editing(false)
                        .prefix("Page ")
                        .suffix(format!(" / {}", document.page_count)),
                )
                .on_hover_text("Click to enter a page number, then press Enter")
                .changed()
            {
                action = Some(ToolbarAction::View(ViewAction::GoToPage(number - 1)));
            }
            if ui
                .add_enabled(
                    document.current_page + 1 < document.page_count,
                    egui::Button::new("Next"),
                )
                .on_hover_text("Page Down")
                .clicked()
            {
                action = Some(ToolbarAction::View(ViewAction::NextPage));
            }
            ui.separator();
            if ui.button("Fit page").on_hover_text("Ctrl+0").clicked() {
                action = Some(ToolbarAction::View(ViewAction::Fit));
            }
            if ui.button("Fit width").on_hover_text("Ctrl+2").clicked() {
                action = Some(ToolbarAction::View(ViewAction::FitWidth));
            }
            if ui.button("100%").on_hover_text("Ctrl+1").clicked() {
                action = Some(ToolbarAction::View(ViewAction::ActualSize));
            }
            if ui.button("-").clicked() {
                action = Some(ToolbarAction::View(ViewAction::Zoom(1.0 / 1.2)));
            }
            ui.add_sized(
                [48.0, 18.0],
                egui::Label::new(format!("{:.0}%", zoom * 100.0)),
            );
            if ui.button("+").clicked() {
                action = Some(ToolbarAction::View(ViewAction::Zoom(1.2)));
            }
            if ui.button("Rotate 90 deg").clicked() {
                action = Some(ToolbarAction::View(ViewAction::Rotate));
            }
            ui.separator();
            ui.label(if document.mouse_pen {
                "Pen/left: write | Middle: pan | Wheel: scroll | Ctrl+wheel: zoom"
            } else {
                "Pen: write | Left/middle: pan | Wheel: scroll | Ctrl+wheel: zoom"
            });
            ui.label("Ctrl+pen/left drag: zoom | Alt+pen/left drag: pan");
        });
        if show_renderer_info {
            renderer_info(ui, page, viewport, fps);
        }
    });
    action
}

fn renderer_info(
    ui: &mut egui::Ui,
    page: PageGeometry,
    viewport: Option<CanvasViewport>,
    fps: Option<f64>,
) {
    let crop = page.crop_box();
    let fps = fps.map_or_else(|| "--".to_owned(), |fps| format!("{fps:.1}"));
    ui.label(format!(
        "Skia + egui | GL | FPS: {fps} / {MAX_FPS} max | CropBox [{:.0}, {:.0}, {:.0}, {:.0}] | UserUnit {} | Rotation {} deg | F12: hide",
        crop.min.x, crop.min.y, crop.max.x, crop.max.y,
        page.user_unit(), page.rotation().degrees(),
    ));
    if let Some(viewport) = viewport {
        let bounds = viewport.physical;
        let logical_size = viewport.physical_to_logical(Point::new(
            f64::from(bounds.right - bounds.left),
            f64::from(bounds.bottom - bounds.top),
        ));
        ui.label(format!(
            "Canvas: {} x {} physical px | {:.2} px/logical point",
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            viewport.pixels_per_point,
        ));
        ui.label(format!(
            "Canvas: {:.1} x {:.1} logical points",
            logical_size.x, logical_size.y,
        ));
    }
}
