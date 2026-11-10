//! Cursor decisions follow actual contact ownership and tool overrides.

use crate::cursor::{CanvasCursor, CursorSettings};

use super::{CanvasInput, CanvasTool};

impl CanvasInput {
    pub fn cursor(
        &self,
        settings: CursorSettings,
        modifiers: egui::Modifiers,
        dragging: bool,
    ) -> CanvasCursor {
        use egui::CursorIcon;
        let owns_canvas = self.mouse_active || self.contact.is_some_and(|(_, _, ui)| !ui);
        if modifiers.ctrl {
            return CanvasCursor::Icon(CursorIcon::ZoomIn);
        }
        let mouse_navigation = self.pen_hover.is_none()
            && !self.mouse_pen
            && matches!(self.tool, CanvasTool::Pen | CanvasTool::Highlighter);
        if modifiers.alt || self.tool == CanvasTool::Pan || mouse_navigation {
            return CanvasCursor::Icon(if owns_canvas || dragging {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            });
        }
        // Middle-button drags are handled by egui, independent of the pen tool.
        if dragging {
            return CanvasCursor::Icon(CursorIcon::Grabbing);
        }
        if self.active.is_some() {
            return if settings.show_drawing {
                CanvasCursor::Style(settings.drawing)
            } else {
                CanvasCursor::Icon(CursorIcon::None)
            };
        }
        let tool = self.object.as_ref().map_or_else(
            || {
                if self.pen_hover.is_some_and(|(_, eraser)| eraser) {
                    CanvasTool::Eraser
                } else {
                    self.tool
                }
            },
            |object| object.tool,
        );
        match tool {
            CanvasTool::Pen | CanvasTool::Highlighter => CanvasCursor::Style(settings.regular),
            CanvasTool::Eraser => CanvasCursor::Eraser,
            CanvasTool::Select | CanvasTool::Lasso => CanvasCursor::Icon(CursorIcon::Crosshair),
            CanvasTool::Pan => unreachable!("pan handled above"),
        }
    }
}
