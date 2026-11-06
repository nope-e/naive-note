//! Central canvas layout and input ownership. Emits gestures in logical coordinates.

use crate::document::{CanvasViewport, Point, Rect};

#[derive(Default)]
pub(super) struct CanvasArea {
    wheel_owned: bool,
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
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        framebuffer: (u32, u32),
        process_input: bool,
        clear_wheel: bool,
    ) -> Option<CanvasResponse> {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
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
                let dragging = response.dragged_by(egui::PointerButton::Primary)
                    || response.dragged_by(egui::PointerButton::Middle);
                let drag_delta = if process_input && dragging {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    let delta = response.drag_delta();
                    Some(Point::new(f64::from(delta.x), f64::from(delta.y)))
                } else {
                    if response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    }
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
                Some(CanvasResponse {
                    viewport,
                    drag_delta,
                    zoom,
                    scroll_delta,
                })
            })
            .inner
    }
}
