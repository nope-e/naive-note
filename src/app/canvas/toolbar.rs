//! Toolbar widgets and shortcuts produce document or view actions.

use crate::{
    app::frame_scheduler::MAX_FPS,
    document::{CanvasViewport, PageGeometry, Point},
};

use super::view::ViewAction;

pub(super) enum ToolbarAction {
    Open,
    Close,
    View(ViewAction),
}

pub(super) fn show(
    ui: &mut egui::Ui,
    zoom: f64,
    page: PageGeometry,
    viewport: Option<CanvasViewport>,
    show_renderer_info: bool,
    fps: Option<f64>,
    has_pdf: bool,
) -> Option<ToolbarAction> {
    let mut action = ui.input(|input| {
        if input.modifiers.ctrl && input.key_pressed(egui::Key::O) {
            Some(ToolbarAction::Open)
        } else if input.modifiers.ctrl && input.key_pressed(egui::Key::W) {
            Some(ToolbarAction::Close)
        } else if input.modifiers.ctrl && input.key_pressed(egui::Key::Num0) {
            Some(ToolbarAction::View(ViewAction::Fit))
        } else if input.modifiers.ctrl && input.key_pressed(egui::Key::Num1) {
            Some(ToolbarAction::View(ViewAction::ActualSize))
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
                .add_enabled(has_pdf, egui::Button::new("Close PDF"))
                .on_hover_text("Ctrl+W")
                .clicked()
            {
                action = Some(ToolbarAction::Close);
            }
            ui.separator();
            if ui.button("Fit page").on_hover_text("Ctrl+0").clicked() {
                action = Some(ToolbarAction::View(ViewAction::Fit));
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
            ui.label("Wheel: scroll | Ctrl+wheel: zoom | Left/middle drag: pan");
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
