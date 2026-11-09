//! Invisible resize edges for the undecorated editor window.

use egui::{CursorIcon, Rect, ResizeDirection, Sense, ViewportCommand, pos2, vec2};

pub(super) fn show(context: &egui::Context, process_input: bool) {
    if context.input(|input| input.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let rect = context.content_rect();
    let (left, top, right, bottom) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    let edge = 5.0;
    let corner = 9.0;
    for (direction, cursor, bounds) in [
        (
            ResizeDirection::North,
            CursorIcon::ResizeVertical,
            Rect::from_min_max(pos2(left + corner, top), pos2(right - corner, top + edge)),
        ),
        (
            ResizeDirection::South,
            CursorIcon::ResizeVertical,
            Rect::from_min_max(
                pos2(left + corner, bottom - edge),
                pos2(right - corner, bottom),
            ),
        ),
        (
            ResizeDirection::West,
            CursorIcon::ResizeHorizontal,
            Rect::from_min_max(pos2(left, top + corner), pos2(left + edge, bottom - corner)),
        ),
        (
            ResizeDirection::East,
            CursorIcon::ResizeHorizontal,
            Rect::from_min_max(
                pos2(right - edge, top + corner),
                pos2(right, bottom - corner),
            ),
        ),
        (
            ResizeDirection::NorthWest,
            CursorIcon::ResizeNwSe,
            Rect::from_min_size(rect.left_top(), vec2(corner, corner)),
        ),
        (
            ResizeDirection::NorthEast,
            CursorIcon::ResizeNeSw,
            Rect::from_min_max(pos2(right - corner, top), pos2(right, top + corner)),
        ),
        (
            ResizeDirection::SouthWest,
            CursorIcon::ResizeNeSw,
            Rect::from_min_max(pos2(left, bottom - corner), pos2(left + corner, bottom)),
        ),
        (
            ResizeDirection::SouthEast,
            CursorIcon::ResizeNwSe,
            Rect::from_min_max(pos2(right - corner, bottom - corner), rect.right_bottom()),
        ),
    ] {
        egui::Area::new(egui::Id::unique(("window_edge", direction as u8)))
            .order(egui::Order::Foreground)
            .fixed_pos(bounds.min)
            .default_size(bounds.size())
            .constrain(false)
            .fade_in(false)
            .movable(false)
            .show(context, |ui| {
                let (_, response) = ui.allocate_exact_size(bounds.size(), Sense::drag());
                let response = response.on_hover_cursor(cursor);
                if process_input && response.drag_started() {
                    context.send_viewport_cmd(ViewportCommand::BeginResize(direction));
                }
            });
    }
}
