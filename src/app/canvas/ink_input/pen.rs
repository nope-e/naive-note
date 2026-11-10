//! Native pen contact routing and Down-time ownership.

use std::sync::Arc;

use tablet_input_windows::{PenPhase, PenSample};

use crate::{
    document::{Point, StrokeRecord},
    ink::LiveStroke,
};

use super::{CanvasInput, CanvasTool, CanvasView};

impl CanvasInput {
    /// Returns whether to route this event to egui, plus a completed stroke.
    pub fn pen(
        &mut self,
        sample: PenSample,
        view: &mut CanvasView,
        enabled: bool,
        modifiers: egui::Modifiers,
    ) -> (bool, Option<Arc<StrokeRecord>>) {
        let position = Point::new(sample.position[0], sample.position[1]);
        if !self.mouse_active
            && self
                .contact
                .is_none_or(|(pointer, _, _)| pointer == sample.pointer_id)
        {
            if matches!(sample.phase, PenPhase::Leave | PenPhase::Cancel) {
                if self
                    .pen_hover
                    .is_some_and(|(pointer, _)| pointer == sample.pointer_id)
                {
                    self.pen_hover = None;
                }
            } else {
                self.pen_hover = Some((sample.pointer_id, sample.eraser || sample.inverted));
            }
        }
        if sample.phase == PenPhase::Down {
            let Some(id) = sample.stroke_id else {
                return (false, None);
            };
            if self.contact.is_some() || self.mouse_active {
                return (false, None);
            }
            let in_canvas = !self.ui_blocked
                && view
                    .viewport
                    .is_some_and(|v| v.logical.contains(v.physical_to_logical(position)));
            let ui_owned = !in_canvas;
            self.contact = Some((sample.pointer_id, id, ui_owned));
            self.last_position = Some(position);
            if !ui_owned && self.begin_navigation(position, view, modifiers) {
                return (false, None);
            }
            let tool = if sample.eraser || sample.inverted {
                CanvasTool::Eraser
            } else {
                self.tool
            };
            if !ui_owned && enabled && self.begin_object(position, view, tool, modifiers.shift) {
                return (false, None);
            }
            if !ui_owned
                && enabled
                && matches!(self.tool, CanvasTool::Pen | CanvasTool::Highlighter)
                && !sample.eraser
                && !sample.inverted
                && let Some(target) = view.stroke_target(position)
            {
                let mut stroke =
                    LiveStroke::new(target, self.smoothing, self.current_style().clone());
                stroke.push_pen(sample);
                self.active = Some(stroke);
            }
            return (ui_owned, None);
        }
        if matches!(
            sample.phase,
            PenPhase::Enter | PenPhase::Hover | PenPhase::Leave
        ) {
            return (self.contact.is_none_or(|(_, _, ui)| ui), None);
        }
        let Some((pointer, id, ui_owned)) = self.contact else {
            return (false, None);
        };
        if (sample.pointer_id, sample.stroke_id) != (pointer, Some(id)) {
            return (false, None);
        }
        self.last_position = Some(position);
        match sample.phase {
            PenPhase::Move => {
                if !ui_owned && self.update_navigation(position, view, modifiers) {
                    self.cancel_object();
                    return (false, self.finish_stroke());
                }
                self.move_object(position, view);
                if let Some(stroke) = self.active.as_mut() {
                    stroke.push_pen(sample);
                }
                (ui_owned, None)
            }
            PenPhase::Up => {
                self.contact = None;
                if !ui_owned && self.update_navigation(position, view, modifiers) {
                    self.navigation = None;
                    self.cancel_object();
                    return (false, self.finish_stroke());
                }
                self.move_object(position, view);
                if self.finish_object() {
                    return (ui_owned, None);
                }
                let completed = self.active.take().and_then(|mut stroke| {
                    stroke.push_pen(sample);
                    stroke.finish()
                });
                (ui_owned, completed)
            }
            PenPhase::Cancel => {
                self.cancel();
                (ui_owned, None)
            }
            _ => (false, None),
        }
    }
}
