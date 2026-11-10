//! Pure input-state checks: no window, egui frame, or cursor image generation.

use super::*;
use crate::{
    app::canvas::ink_input::CanvasTool,
    cursor::{CanvasCursor, CursorSettings},
};
use egui::{CursorIcon, Modifiers};

#[test]
fn actual_ink_switches_hover_and_drawing_cursors_for_mouse_and_pen() {
    let settings = CursorSettings::default();
    for mouse in [false, true] {
        for tool in [CanvasTool::Pen, CanvasTool::Highlighter] {
            let (mut input, mut view, position) = setup();
            input.tool = tool;
            input.mouse_pen = true;
            assert_eq!(
                input.cursor(settings, Modifiers::NONE, false),
                CanvasCursor::Style(settings.regular)
            );
            if mouse {
                input.mouse_button(true, Some(position), &mut view, true, Modifiers::NONE);
            } else {
                input.pen(
                    sample(PenPhase::Down, position),
                    &mut view,
                    true,
                    Modifiers::NONE,
                );
            }
            assert_eq!(
                input.cursor(settings, Modifiers::NONE, false),
                CanvasCursor::Style(settings.drawing)
            );
            let hidden = CursorSettings {
                show_drawing: false,
                ..settings
            };
            assert_eq!(
                input.cursor(hidden, Modifiers::NONE, false),
                CanvasCursor::Icon(CursorIcon::None)
            );
            if mouse {
                input.mouse_button(false, Some(position), &mut view, true, Modifiers::NONE);
            } else {
                input.pen(
                    sample(PenPhase::Up, position),
                    &mut view,
                    true,
                    Modifiers::NONE,
                );
            }
            assert_eq!(
                input.cursor(hidden, Modifiers::NONE, false),
                CanvasCursor::Style(settings.regular)
            );
        }
    }
}

#[test]
fn navigation_and_middle_drag_override_hidden_drawing_cursor() {
    let settings = CursorSettings {
        show_drawing: false,
        ..Default::default()
    };
    let (mut input, mut view, position) = setup();
    input.mouse_pen = true;
    input.mouse_button(true, Some(position), &mut view, true, Modifiers::NONE);
    assert_eq!(
        input.cursor(settings, Modifiers::CTRL, false),
        CanvasCursor::Icon(CursorIcon::ZoomIn)
    );
    assert_eq!(
        input.cursor(settings, Modifiers::ALT, false),
        CanvasCursor::Icon(CursorIcon::Grabbing)
    );
    input.cancel();
    assert_eq!(
        input.cursor(settings, Modifiers::ALT, false),
        CanvasCursor::Icon(CursorIcon::Grab)
    );
    assert_eq!(
        input.cursor(settings, Modifiers::NONE, true),
        CanvasCursor::Icon(CursorIcon::Grabbing)
    );
    for tool in [CanvasTool::Pan, CanvasTool::Select, CanvasTool::Eraser] {
        input.tool = tool;
        assert_eq!(
            input.cursor(settings, Modifiers::CTRL, false),
            CanvasCursor::Icon(CursorIcon::ZoomIn)
        );
        assert_eq!(
            input.cursor(settings, Modifiers::ALT, false),
            CanvasCursor::Icon(CursorIcon::Grab)
        );
    }
}

#[test]
fn pen_proximity_and_eraser_override_mouse_navigation_until_mouse_moves() {
    let settings = CursorSettings::default();
    let (mut input, mut view, position) = setup();
    input.mouse_pen = false;
    assert_eq!(
        input.cursor(settings, Modifiers::NONE, false),
        CanvasCursor::Icon(CursorIcon::Grab)
    );
    input.pen(
        sample(PenPhase::Hover, position),
        &mut view,
        true,
        Modifiers::NONE,
    );
    assert_eq!(
        input.cursor(settings, Modifiers::NONE, false),
        CanvasCursor::Style(settings.regular)
    );
    let mut eraser = sample(PenPhase::Hover, position);
    eraser.inverted = true;
    input.pen(eraser, &mut view, true, Modifiers::NONE);
    assert_eq!(
        input.cursor(settings, Modifiers::NONE, false),
        CanvasCursor::Eraser
    );
    input.tool = CanvasTool::Pan;
    assert_eq!(
        input.cursor(settings, Modifiers::NONE, false),
        CanvasCursor::Icon(CursorIcon::Grab)
    );
    input.tool = CanvasTool::Pen;
    input.mouse_move(position, &mut view, Modifiers::NONE);
    assert_eq!(
        input.cursor(settings, Modifiers::NONE, false),
        CanvasCursor::Icon(CursorIcon::Grab)
    );
    input.pen(eraser, &mut view, true, Modifiers::NONE);
    eraser.phase = PenPhase::Leave;
    input.pen(eraser, &mut view, true, Modifiers::NONE);
    assert_eq!(
        input.cursor(settings, Modifiers::NONE, false),
        CanvasCursor::Icon(CursorIcon::Grab)
    );
}

#[test]
fn object_tools_and_ui_contacts_never_use_the_hidden_ink_cursor() {
    let settings = CursorSettings {
        show_drawing: false,
        ..Default::default()
    };
    let (mut input, mut view, position) = setup();
    for (tool, expected) in [
        (
            CanvasTool::Select,
            CanvasCursor::Icon(CursorIcon::Crosshair),
        ),
        (CanvasTool::Lasso, CanvasCursor::Icon(CursorIcon::Crosshair)),
        (CanvasTool::Eraser, CanvasCursor::Eraser),
    ] {
        input.tool = tool;
        assert_eq!(input.cursor(settings, Modifiers::NONE, false), expected);
        input.mouse_button(true, Some(position), &mut view, true, Modifiers::NONE);
        assert_eq!(input.cursor(settings, Modifiers::NONE, false), expected);
        input.cancel();
    }
    input.tool = CanvasTool::Pen;
    input.mouse_pen = true;
    input.ui_blocked = true;
    input.pen(
        sample(PenPhase::Down, position),
        &mut view,
        true,
        Modifiers::NONE,
    );
    assert_eq!(
        input.cursor(settings, Modifiers::NONE, false),
        CanvasCursor::Style(settings.regular)
    );
    input.cancel();
    input.ui_blocked = false;
    input.mouse_button(true, Some(position), &mut view, true, Modifiers::NONE);
    input.cancel();
    assert_eq!(
        input.cursor(settings, Modifiers::NONE, false),
        CanvasCursor::Style(settings.regular)
    );
}
