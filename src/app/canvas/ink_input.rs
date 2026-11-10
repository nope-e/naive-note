//! Shared contact ownership and stroke lifetime; no native calls or rendering.

mod cursor;
mod mouse;
mod navigation;
mod objects;
mod pen;
#[cfg(test)]
mod tests;

use std::{sync::Arc, time::Instant};

use crate::{
    document::{PenStyle, Point, StrokeRecord},
    ink::{ActiveInk, LiveStroke},
};

use super::view::CanvasView;
use navigation::Navigation;
pub(super) use objects::{ObjectEvent, ObjectGesture};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum CanvasTool {
    #[default]
    Pen,
    Highlighter,
    Eraser,
    Select,
    Lasso,
    Pan,
}

impl CanvasTool {
    pub fn is_selection(self) -> bool {
        matches!(self, Self::Select | Self::Lasso)
    }
}

pub(super) struct CanvasInput {
    active: Option<LiveStroke>,
    // Ownership is fixed on Down, including contacts begun in gaps/UI.
    contact: Option<(u32, u64, bool)>,
    mouse_active: bool,
    navigation: Option<Navigation>,
    object: Option<ObjectGesture>,
    object_events: Vec<ObjectEvent>,
    last_position: Option<Point>,
    // Hovering pen erasers temporarily override the selected drawing tool.
    pen_hover: Option<(u32, bool)>,
    pub tool: CanvasTool,
    pub mouse_pen: bool,
    pub smoothing: bool,
    pub style: PenStyle,
    pub highlighter_style: PenStyle,
    // Only consulted on Down; UI/canvas ownership then remains fixed until Up.
    pub ui_blocked: bool,
    clock: Instant,
}

impl Default for CanvasInput {
    fn default() -> Self {
        Self {
            active: None,
            contact: None,
            mouse_active: false,
            navigation: None,
            object: None,
            object_events: Vec::new(),
            last_position: None,
            pen_hover: None,
            tool: CanvasTool::default(),
            mouse_pen: false,
            smoothing: true,
            style: PenStyle::default(),
            highlighter_style: PenStyle::highlighter(),
            ui_blocked: false,
            clock: Instant::now(),
        }
    }
}

impl CanvasInput {
    pub fn current_style(&self) -> &PenStyle {
        if self.tool == CanvasTool::Highlighter {
            &self.highlighter_style
        } else {
            &self.style
        }
    }

    pub fn current_style_mut(&mut self) -> &mut PenStyle {
        if self.tool == CanvasTool::Highlighter {
            &mut self.highlighter_style
        } else {
            &mut self.style
        }
    }

    pub fn for_editor() -> Self {
        Self {
            mouse_pen: true,
            style: PenStyle {
                color_rgb: [0, 0, 0],
                max_width_pt: 4.6,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// A keyboard tool switch ends the logical stroke at its last ink sample.
    /// The physical contact continues as navigation until Up/Cancel.
    fn finish_stroke(&mut self) -> Option<Arc<StrokeRecord>> {
        self.active.take().and_then(LiveStroke::finish)
    }

    fn time_us(&self) -> u64 {
        self.clock.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
    }

    pub fn is_drawing(&self) -> bool {
        self.active.is_some() || self.object.is_some()
    }

    pub fn active(&self) -> Option<ActiveInk> {
        self.active.as_ref().map(LiveStroke::snapshot)
    }

    pub fn last_ink_sample(&self) -> Option<(u32, Point)> {
        self.active.as_ref().and_then(LiveStroke::last_ink_sample)
    }

    pub fn cancel(&mut self) {
        self.cancel_object();
        self.active = None;
        self.contact = None;
        self.mouse_active = false;
        self.navigation = None;
        self.last_position = None;
        self.pen_hover = None;
    }

    pub fn finish_for_save(&mut self) -> Option<Arc<StrokeRecord>> {
        let stroke = self.finish_stroke();
        self.cancel_drawing();
        stroke
    }

    /// UI drags must retain ownership until Up, even when they change the view.
    pub fn cancel_drawing(&mut self) {
        self.cancel_object();
        self.active = None;
        self.mouse_active = false;
        self.navigation = None;
        if self.contact.is_some_and(|(_, _, ui_owned)| !ui_owned) {
            self.contact = None;
        }
    }
}
