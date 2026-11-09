//! Pure ownership/style checks; no egui frame, window, or rendered pixels.

use super::*;

#[test]
fn floating_ui_contact_keeps_ownership_when_dragged_onto_paper() {
    for modifiers in [
        egui::Modifiers::NONE,
        egui::Modifiers::CTRL,
        egui::Modifiers::ALT,
    ] {
        let (mut input, mut view, position) = setup();
        let before = view.transform;
        input.ui_blocked = true;
        assert!(
            input
                .pen(sample(PenPhase::Down, position), &mut view, true, modifiers)
                .0
        );
        assert!(!input.is_drawing());
        input.ui_blocked = false;
        let end = Point::new(position.x + 25.0, position.y + 20.0);
        assert!(
            input
                .pen(sample(PenPhase::Move, end), &mut view, true, modifiers)
                .0
        );
        let (ui, stroke) = input.pen(sample(PenPhase::Up, end), &mut view, true, modifiers);
        assert!(ui && stroke.is_none());
        assert_eq!(view.transform, before);
    }
}

#[test]
fn floating_ui_mouse_down_never_starts_ink_or_modifier_navigation() {
    for modifiers in [
        egui::Modifiers::NONE,
        egui::Modifiers::CTRL,
        egui::Modifiers::ALT,
    ] {
        let (mut input, mut view, position) = setup();
        input.mouse_pen = true;
        input.ui_blocked = true;
        let before = view.transform;
        assert!(
            !input
                .mouse_button(true, Some(position), &mut view, true, modifiers)
                .0
        );
        input.ui_blocked = false;
        let end = Point::new(position.x + 20.0, position.y + 20.0);
        assert!(input.mouse_move(end, &mut view, modifiers).is_none());
        let (owned, stroke) = input.mouse_button(false, Some(end), &mut view, true, modifiers);
        assert!(!owned && stroke.is_none());
        assert_eq!(view.transform, before);
    }
}

#[test]
fn pen_and_mouse_freeze_color_width_and_pressure_at_down() {
    for mouse in [false, true] {
        let (mut input, mut view, position) = setup();
        input.mouse_pen = mouse;
        input.style.color_rgb = [237, 51, 65];
        input.style.max_width_pt = 9.0;
        let original = input.style.clone();
        if mouse {
            input.mouse_button(true, Some(position), &mut view, true, egui::Modifiers::NONE);
        } else {
            input.pen(
                sample(PenPhase::Down, position),
                &mut view,
                true,
                egui::Modifiers::NONE,
            );
        }
        input.style.color_rgb = [32, 100, 178];
        input.style.max_width_pt = 1.0;
        input.style.pressure_gamma = 2.0;
        // Crossing a floating control also cannot steal an existing stroke.
        input.ui_blocked = true;
        let first = if mouse {
            input
                .mouse_button(
                    false,
                    Some(position),
                    &mut view,
                    true,
                    egui::Modifiers::NONE,
                )
                .1
                .unwrap()
        } else {
            input
                .pen(
                    sample(PenPhase::Up, position),
                    &mut view,
                    true,
                    egui::Modifiers::NONE,
                )
                .1
                .unwrap()
        };
        assert_eq!(first.style, original);
        input.ui_blocked = false;
        let second = if mouse {
            input.mouse_button(true, Some(position), &mut view, true, egui::Modifiers::NONE);
            input
                .mouse_button(
                    false,
                    Some(position),
                    &mut view,
                    true,
                    egui::Modifiers::NONE,
                )
                .1
                .unwrap()
        } else {
            input.pen(
                sample(PenPhase::Down, position),
                &mut view,
                true,
                egui::Modifiers::NONE,
            );
            input
                .pen(
                    sample(PenPhase::Up, position),
                    &mut view,
                    true,
                    egui::Modifiers::NONE,
                )
                .1
                .unwrap()
        };
        assert_eq!(second.style, input.style);
        assert_eq!(first.style, original);
    }
}
