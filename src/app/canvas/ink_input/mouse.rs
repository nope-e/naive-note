//! Optional fixed-pressure mouse strokes and left-button navigation.

use std::sync::Arc;

use crate::{
    document::{PenPhase as StrokePhase, Point, StrokeRecord},
    ink::LiveStroke,
};

use super::{CanvasInput, CanvasTool, CanvasView};

impl CanvasInput {
    pub fn mouse_button(
        &mut self,
        down: bool,
        position: Option<Point>,
        view: &mut CanvasView,
        enabled: bool,
        modifiers: egui::Modifiers,
    ) -> (bool, Option<Arc<StrokeRecord>>) {
        if down {
            if self.ui_blocked {
                return (false, None);
            }
            if self.contact.is_some() || self.mouse_active {
                return (false, None);
            }
            self.pen_hover = None;
            let Some(position) = position else {
                return (false, None);
            };
            if view
                .viewport
                .is_some_and(|v| v.logical.contains(v.physical_to_logical(position)))
                && self.begin_navigation(position, view, modifiers)
            {
                self.mouse_active = true;
                self.last_position = Some(position);
                return (true, None);
            }
            if !enabled {
                return (false, None);
            }
            if self.begin_object(position, view, self.tool, modifiers.shift) {
                self.mouse_active = true;
                self.last_position = Some(position);
                return (true, None);
            }
            if !self.mouse_pen || !matches!(self.tool, CanvasTool::Pen | CanvasTool::Highlighter) {
                return (false, None);
            }
            let Some(target) = view.stroke_target(position) else {
                return (false, None);
            };
            let mut stroke = LiveStroke::new(target, self.smoothing, self.current_style().clone());
            stroke.push_mouse(position, self.time_us(), StrokePhase::Down);
            self.active = Some(stroke);
            self.mouse_active = true;
            self.last_position = Some(position);
            (true, None)
        } else if self.mouse_active {
            self.mouse_active = false;
            if let Some(position) = position {
                self.update_navigation(position, view, modifiers);
            }
            if self.navigation.take().is_some() {
                self.cancel_object();
                return (true, self.finish_stroke());
            }
            if let Some(position) = position {
                self.move_object(position, view);
            }
            if self.finish_object() {
                return (true, None);
            }
            let time = self.time_us();
            let completed = self.active.take().and_then(|mut stroke| {
                if let Some(position) = position {
                    stroke.push_mouse(position, time, StrokePhase::Up);
                }
                stroke.finish()
            });
            (true, completed)
        } else {
            (false, None)
        }
    }

    pub fn mouse_move(
        &mut self,
        position: Point,
        view: &mut CanvasView,
        modifiers: egui::Modifiers,
    ) -> Option<Arc<StrokeRecord>> {
        if self.contact.is_none() {
            self.pen_hover = None;
        }
        if !self.mouse_active {
            return None;
        }
        let time = self.time_us();
        self.last_position = Some(position);
        if self.update_navigation(position, view, modifiers) {
            self.cancel_object();
            return self.finish_stroke();
        }
        self.move_object(position, view);
        if let Some(stroke) = self.active.as_mut() {
            stroke.push_mouse(position, time, StrokePhase::Move);
        }
        None
    }
}
