//! Shared contact ownership and stroke lifetime; no native calls or rendering.

mod mouse;
mod navigation;
mod pen;
#[cfg(test)]
mod tests;

use std::{sync::Arc, time::Instant};

use crate::{
    document::{Point, StrokeRecord},
    ink::{ActiveInk, LiveStroke},
};

use super::view::CanvasView;
use navigation::Navigation;

pub(super) struct CanvasInput {
    active: Option<LiveStroke>,
    // Ownership is fixed on Down, including contacts begun in gaps/UI.
    contact: Option<(u32, u64, bool)>,
    mouse_active: bool,
    navigation: Option<Navigation>,
    last_position: Option<Point>,
    pub mouse_pen: bool,
    pub smoothing: bool,
    clock: Instant,
}

impl Default for CanvasInput {
    fn default() -> Self {
        Self {
            active: None,
            contact: None,
            mouse_active: false,
            navigation: None,
            last_position: None,
            mouse_pen: false,
            smoothing: true,
            clock: Instant::now(),
        }
    }
}

impl CanvasInput {
    /// A keyboard tool switch ends the logical stroke at its last ink sample.
    /// The physical contact continues as navigation until Up/Cancel.
    fn finish_stroke(&mut self) -> Option<Arc<StrokeRecord>> {
        self.active.take().and_then(LiveStroke::finish)
    }

    fn time_us(&self) -> u64 {
        self.clock.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
    }

    pub fn is_drawing(&self) -> bool {
        self.active.is_some()
    }

    pub fn active(&self) -> Option<ActiveInk> {
        self.active.as_ref().map(LiveStroke::snapshot)
    }

    pub fn cancel(&mut self) {
        self.active = None;
        self.contact = None;
        self.mouse_active = false;
        self.navigation = None;
        self.last_position = None;
    }

    /// UI drags must retain ownership until Up, even when they change the view.
    pub fn cancel_drawing(&mut self) {
        self.active = None;
        self.mouse_active = false;
        self.navigation = None;
        if self.contact.is_some_and(|(_, _, ui_owned)| !ui_owned) {
            self.contact = None;
        }
    }
}
