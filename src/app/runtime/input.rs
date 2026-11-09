//! Bridge native pen, mouse and keyboard events to the canvas and egui.

use winit::event::{ElementState, MouseButton, WindowEvent};

use crate::document::Point;

use super::Runtime;

impl Runtime {
    /// Returns true when canvas ownership consumes the event before egui.
    pub(in crate::app) fn handle_input(&mut self, event: &WindowEvent) -> bool {
        // Keep mouse debugging on the same stroke path. The native hook already
        // removed pen-promoted mouse messages, so these cannot duplicate ink.
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let point = Point::new(position.x, position.y);
                self.mouse_position = Some(point);
                self.canvas.mouse_move(point, self.modifiers);
                if self.mouse_owned {
                    self.request_repaint();
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let down = *state == ElementState::Pressed;
                let owned = self.canvas.mouse_button(
                    down,
                    self.mouse_position,
                    self.modifiers,
                    self.mouse_position
                        .is_some_and(|point| self.renderer.ui_contains_point(point)),
                );
                if owned || (!down && self.mouse_owned) {
                    self.mouse_owned = down;
                    self.request_repaint();
                    return true;
                }
            }
            WindowEvent::ModifiersChanged(state) => {
                let state = state.state();
                self.modifiers = egui::Modifiers {
                    ctrl: state.control_key(),
                    alt: state.alt_key(),
                    shift: state.shift_key(),
                    command: state.control_key(),
                    mac_cmd: false,
                };
                self.canvas.modifiers_changed(self.modifiers);
            }
            WindowEvent::Focused(false) => {
                self.modifiers = egui::Modifiers::NONE;
                self.cancel_input();
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => self.cancel_input(),
            _ => {}
        }

        false
    }

    pub(in crate::app) fn drain_pen(&mut self) {
        let samples = self.pen.take_samples();
        if !samples.is_empty() {
            self.request_repaint();
        }
        for sample in samples {
            let over_ui = self.renderer.ui_contains_point(Point::new(
                sample.sample.position[0],
                sample.sample.position[1],
            ));
            if self.canvas.pen_sample(sample, over_ui) {
                self.renderer.pen_ui_event(sample);
            }
        }
        if self.pen.take_cancel_request() {
            self.canvas.cancel_input();
            self.mouse_owned = false;
            self.renderer.cancel_pen_ui();
            self.request_repaint();
        }
    }

    pub(in crate::app) fn cancel_input(&mut self) {
        self.pen.cancel();
        self.pen.take_samples();
        self.pen.take_cancel_request();
        self.canvas.cancel_input();
        self.mouse_owned = false;
        self.renderer.cancel_pen_ui();
        self.request_repaint();
    }
}
