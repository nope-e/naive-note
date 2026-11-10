//! Selection, transform previews and atomic eraser gestures over immutable ink.

mod selection;

use super::{
    document::CanvasDocument,
    ink_input::{CanvasTool, ObjectEvent, ObjectGesture},
};
use crate::document::{Point, Rect};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Default)]
pub(super) struct ObjectEditing {
    selected: HashSet<Uuid>,
    erased: HashSet<Uuid>,
    gesture: Option<ObjectGesture>,
    selection_before: Option<HashSet<Uuid>>,
    selection_gesture: Option<selection::SelectionGesture>,
}

impl ObjectEditing {
    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }

    pub fn selected_unverified_count(&self, document: &CanvasDocument) -> usize {
        document.unverified_count(&self.selected)
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn prune(&mut self, document: &CanvasDocument) {
        self.selected.retain(|id| document.contains_stroke(*id));
    }

    pub fn delete_selected(&mut self, document: &mut CanvasDocument) {
        document.remove_strokes(&self.selected);
        self.selected.clear();
    }

    pub fn select_page(&mut self, document: &CanvasDocument, page: u32) {
        self.selected = document.strokes(page).iter().map(|s| s.id).collect();
    }

    pub fn receive(&mut self, events: Vec<ObjectEvent>, document: &mut CanvasDocument) {
        for event in events {
            match event {
                ObjectEvent::Update(gesture) => self.update(gesture, document),
                ObjectEvent::Finish => {
                    self.finish_selection(document);
                    document.remove_strokes(&self.erased);
                    self.erased.clear();
                    self.gesture = None;
                    self.selection_before = None;
                    self.selection_gesture = None;
                    self.prune(document);
                }
                ObjectEvent::Cancel => {
                    self.erased.clear();
                    self.gesture = None;
                    self.selection_gesture = None;
                    if let Some(before) = self.selection_before.take() {
                        self.selected = before;
                    }
                }
            }
        }
    }

    fn update(&mut self, gesture: ObjectGesture, document: &CanvasDocument) {
        let strokes = document.strokes(gesture.target.page_index);
        let scale = gesture.target.view.scale();
        if gesture.tool == CanvasTool::Eraser {
            let size = gesture.target.original.paper_size();
            // The sweep is clipped to its Down-time page; off-page ink is hidden.
            if let Some((from, to)) = clip_segment(
                gesture.previous,
                gesture.position,
                Rect::new(0., 0., size.width, size.height),
            ) {
                for stroke in strokes {
                    if !self.erased.contains(&stroke.id)
                        && stroke.hit_segment(from, to, 8.0 / scale)
                    {
                        self.erased.insert(stroke.id);
                    }
                }
            }
        } else {
            self.update_selection(gesture, document);
        }
        self.gesture = Some(gesture);
    }
}

fn drag_rect(gesture: ObjectGesture) -> Rect {
    Rect::new(
        gesture.start.x.min(gesture.position.x),
        gesture.start.y.min(gesture.position.y),
        gesture.start.x.max(gesture.position.x),
        gesture.start.y.max(gesture.position.y),
    )
}

/// Liang-Barsky clipping, including taps and sweeps entering from outside.
fn clip_segment(a: Point, b: Point, rect: Rect) -> Option<(Point, Point)> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let (mut enter, mut leave) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-dx, a.x - rect.min.x),
        (dx, rect.max.x - a.x),
        (-dy, a.y - rect.min.y),
        (dy, rect.max.y - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else if p < 0.0 {
            enter = enter.max(q / p);
        } else {
            leave = leave.min(q / p);
        }
        if enter > leave {
            return None;
        }
    }
    Some((
        Point::new(a.x + enter * dx, a.y + enter * dy),
        Point::new(a.x + leave * dx, a.y + leave * dy),
    ))
}

#[cfg(test)]
mod tests;
