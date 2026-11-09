//! Selected-tool and Ctrl/Alt navigation, including transitions from ink.

use std::sync::Arc;

use crate::document::{Point, StrokeRecord};

use super::super::area::{CanvasResponse, ZoomGesture};
use super::{CanvasInput, CanvasTool, CanvasView};

#[derive(Clone, Copy, PartialEq, Eq)]
enum NavigationMode {
    Pan,
    Zoom,
}

pub(super) struct Navigation {
    mode: Option<NavigationMode>,
    previous: Point,
    anchor: Point,
    viewport: crate::document::CanvasViewport,
}

fn navigation_mode(modifiers: egui::Modifiers, tool: CanvasTool) -> Option<NavigationMode> {
    if modifiers.ctrl {
        Some(NavigationMode::Zoom)
    } else if modifiers.alt || tool == CanvasTool::Pan {
        Some(NavigationMode::Pan)
    } else {
        None
    }
}

impl CanvasInput {
    pub fn modifiers_changed(
        &mut self,
        modifiers: egui::Modifiers,
        view: &mut CanvasView,
    ) -> Option<Arc<StrokeRecord>> {
        let owns_canvas = self.mouse_active || self.contact.is_some_and(|(_, _, ui)| !ui);
        if owns_canvas
            && let Some(position) = self.last_position
            && self.update_navigation(position, view, modifiers)
        {
            return self.finish_stroke();
        }
        None
    }

    pub(super) fn begin_navigation(
        &mut self,
        position: Point,
        view: &CanvasView,
        modifiers: egui::Modifiers,
    ) -> bool {
        let Some(mode) = navigation_mode(modifiers, self.tool) else {
            return false;
        };
        let Some(viewport) = view.viewport else {
            return false;
        };
        let logical = viewport.physical_to_logical(position);
        self.navigation = Some(Navigation {
            mode: Some(mode),
            previous: logical,
            anchor: logical,
            viewport,
        });
        true
    }

    pub(super) fn update_navigation(
        &mut self,
        position: Point,
        view: &mut CanvasView,
        modifiers: egui::Modifiers,
    ) -> bool {
        if self.navigation.is_none() {
            return self.begin_navigation(position, view, modifiers);
        }
        let nav = self.navigation.as_mut().unwrap();
        let logical = nav.viewport.physical_to_logical(position);
        let mode = navigation_mode(modifiers, self.tool);
        let delta = Point::new(logical.x - nav.previous.x, logical.y - nav.previous.y);
        nav.previous = logical;
        if nav.mode != mode {
            // Start a fresh anchor when switching tools; no jump or connecting ink.
            nav.mode = mode;
            nav.anchor = logical;
            return true;
        }
        let (drag_delta, zoom) = match mode {
            Some(NavigationMode::Pan) if delta != Point::default() => (Some(delta), None),
            Some(NavigationMode::Zoom) if delta.y != 0.0 => (
                None,
                Some(ZoomGesture {
                    factor: (-delta.y / 200.0).clamp(-4.0, 4.0).exp(),
                    anchor: nav.anchor,
                }),
            ),
            _ => return true,
        };
        view.apply_interaction(CanvasResponse {
            viewport: nav.viewport,
            drag_delta,
            zoom,
            scroll_delta: None,
        });
        true
    }
}
