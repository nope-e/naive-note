//! Central canvas layout and input ownership. Routes logical gestures to the view.

use crate::document::{CanvasViewport, Point, Rect};

use super::{
    scrollbars::Scrollbars,
    view::{CanvasView, ViewAction},
};

#[derive(Default)]
pub(super) struct CanvasArea {
    wheel_owned: bool,
    scrollbars: Scrollbars,
    pointer_over_canvas: bool,
    dragging: bool,
    cursors: crate::cursor::CursorImages,
}

pub(super) struct ZoomGesture {
    pub factor: f64,
    pub anchor: Point,
}

pub(super) struct CanvasResponse {
    pub viewport: CanvasViewport,
    pub drag_delta: Option<Point>,
    pub zoom: Option<ZoomGesture>,
    pub scroll_delta: Option<Point>,
}

impl CanvasArea {
    pub fn dragging(&self) -> bool {
        self.dragging
    }

    pub fn update_cursor(
        &mut self,
        context: &egui::Context,
        cursor: Option<crate::cursor::CanvasCursor>,
    ) {
        // The bitmap is sticky in egui, so explicitly clear it over chrome,
        // popups, modals, after cancellation, and when there is no document.
        context.set_cursor_image(None);
        if self.pointer_over_canvas
            && context.input(|input| input.raw.focused)
            && !egui::Popup::is_any_open(context)
            && let Some(cursor) = cursor
        {
            self.cursors.apply(context, cursor);
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        framebuffer: (u32, u32),
        process_input: bool,
        view: &mut CanvasView,
        action: Option<ViewAction>,
    ) -> Option<CanvasViewport> {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let (outer, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
                // Keep both lanes reserved so showing/hiding a thumb cannot
                // change the viewport or move the pointer's zoom anchor.
                let thickness = ui
                    .spacing()
                    .scroll
                    .bar_width
                    .max(12.0)
                    .min(outer.width() * 0.5)
                    .min(outer.height() * 0.5);
                let rect = egui::Rect::from_min_max(
                    outer.min + egui::vec2(0.0, 54.0_f32.min(outer.height() * 0.25)),
                    outer.max - egui::vec2(thickness, thickness),
                );
                let response = ui.interact(
                    rect,
                    ui.scope_id().with("canvas_area"),
                    egui::Sense::click_and_drag(),
                );
                self.pointer_over_canvas = response.hovered();
                self.dragging = response.dragged_by(egui::PointerButton::Primary)
                    || response.dragged_by(egui::PointerButton::Middle);
                let logical = Rect::new(
                    f64::from(rect.min.x),
                    f64::from(rect.min.y),
                    f64::from(rect.max.x),
                    f64::from(rect.max.y),
                );
                let viewport = CanvasViewport::new(
                    logical,
                    f64::from(ui.ctx().pixels_per_point()),
                    framebuffer,
                )?;
                view.update_viewport(viewport);
                let clear_wheel = action.is_some();
                if process_input && let Some(action) = action {
                    view.apply_action(action, viewport.logical);
                }

                if process_input {
                    if clear_wheel {
                        self.wheel_owned = false;
                    }
                    let starts_wheel = ui.input(|input| {
                        input.events.iter().any(|event| {
                            matches!(event, egui::Event::MouseWheel { .. } | egui::Event::Zoom(_))
                        })
                    });
                    if starts_wheel {
                        self.wheel_owned = response.hovered();
                    }
                    if !ui.input(|input| input.raw.focused) {
                        self.wheel_owned = false;
                    }
                }

                // egui retains ownership when dragging outside the canvas; a
                // gesture begun on another widget cannot become a canvas drag.
                let drag_delta = if process_input && self.dragging {
                    let delta = response.drag_delta();
                    Some(Point::new(f64::from(delta.x), f64::from(delta.y)))
                } else {
                    None
                };

                let (zoom, scroll_delta) =
                    if process_input && self.wheel_owned && response.hovered() {
                        ui.input(|input| {
                            let Some(pointer) = input.pointer.hover_pos() else {
                                return (None, None);
                            };
                            let factor = f64::from(input.zoom_delta());
                            let zoom =
                                ((factor - 1.0).abs() > f64::EPSILON).then_some(ZoomGesture {
                                    factor,
                                    anchor: Point::new(f64::from(pointer.x), f64::from(pointer.y)),
                                });
                            let delta = input.smooth_scroll_delta();
                            let scroll = (delta != egui::Vec2::ZERO)
                                .then_some(Point::new(f64::from(delta.x), f64::from(delta.y)));
                            (zoom, scroll)
                        })
                    } else {
                        (None, None)
                    };
                view.apply_interaction(CanvasResponse {
                    viewport,
                    drag_delta,
                    zoom,
                    scroll_delta,
                });
                let tracks = [
                    egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, rect.max.y),
                        egui::pos2(rect.max.x, outer.max.y),
                    ),
                    egui::Rect::from_min_max(
                        egui::pos2(rect.max.x, rect.min.y),
                        egui::pos2(outer.max.x, rect.max.y),
                    ),
                ];
                let offsets = self.scrollbars.show(
                    ui,
                    tracks,
                    view.scroll_axes(viewport.logical),
                    process_input,
                );
                view.apply_scroll_offsets(viewport.logical, offsets);
                Some(viewport)
            })
            .inner
    }
}
