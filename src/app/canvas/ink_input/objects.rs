//! Object gestures use the same Down-time ownership and frozen page mapping as ink.

use super::{CanvasInput, CanvasTool, CanvasView};
use crate::{document::Point, ink::StrokeTarget};

#[derive(Clone, Copy)]
pub(in crate::app::canvas) struct ObjectGesture {
    pub target: StrokeTarget,
    /// Paper under the current pointer; ownership and lasso/rotation stay bound
    /// to target, while a move can transfer its group here when released.
    pub destination: Option<StrokeTarget>,
    pub tool: CanvasTool,
    pub start: Point,
    pub previous: Point,
    pub position: Point,
    pub additive: bool,
}

#[expect(
    clippy::large_enum_variant,
    reason = "Events are drained after each input; avoid boxing every sampled gesture"
)]
pub(in crate::app::canvas) enum ObjectEvent {
    Update(ObjectGesture),
    Finish,
    Cancel,
}

impl CanvasInput {
    pub fn take_object_events(&mut self) -> Vec<ObjectEvent> {
        std::mem::take(&mut self.object_events)
    }

    pub(super) fn begin_object(
        &mut self,
        position: Point,
        view: &CanvasView,
        tool: CanvasTool,
        additive: bool,
    ) -> bool {
        if tool != CanvasTool::Eraser && !tool.is_selection() {
            return false;
        }
        let Some(target) = view.stroke_target(position) else {
            return false;
        };
        let position = target.paper_position(position);
        let gesture = ObjectGesture {
            target,
            destination: Some(target),
            tool,
            start: position,
            previous: position,
            position,
            additive,
        };
        self.object = Some(gesture);
        self.object_events.push(ObjectEvent::Update(gesture));
        true
    }

    pub(super) fn move_object(&mut self, position: Point, view: &CanvasView) {
        if let Some(gesture) = &mut self.object {
            let destination = view.stroke_target(position);
            let position = gesture.target.paper_position(position);
            if !position.is_finite() {
                return;
            }
            gesture.previous = gesture.position;
            gesture.position = position;
            gesture.destination = destination;
            self.object_events.push(ObjectEvent::Update(*gesture));
        }
    }

    pub(super) fn finish_object(&mut self) -> bool {
        if self.object.take().is_none() {
            return false;
        }
        self.object_events.push(ObjectEvent::Finish);
        true
    }

    pub(super) fn cancel_object(&mut self) {
        if self.object.take().is_some() {
            self.object_events.push(ObjectEvent::Cancel);
        }
    }
}
