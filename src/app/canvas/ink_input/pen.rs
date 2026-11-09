//! Native pen contact routing and Down-time ownership.

use std::sync::Arc;

use tablet_input_windows::{PenPhase, PenSample};

use crate::{
    document::{Point, StrokeRecord},
    ink::LiveStroke,
};

use super::{CanvasInput, CanvasView};

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
            if !ui_owned
                && enabled
                && !sample.eraser
                && !sample.inverted
                && let Some(target) = view.stroke_target(position)
            {
                let mut stroke = LiveStroke::new(target, self.smoothing, self.style.clone());
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
                    return (false, self.finish_stroke());
                }
                if let Some(stroke) = self.active.as_mut() {
                    stroke.push_pen(sample);
                }
                (ui_owned, None)
            }
            PenPhase::Up => {
                self.contact = None;
                if !ui_owned && self.update_navigation(position, view, modifiers) {
                    self.navigation = None;
                    return (false, self.finish_stroke());
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
