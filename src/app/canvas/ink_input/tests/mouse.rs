use super::*;

#[test]
fn mouse_debug_is_explicit_fixed_pressure_and_does_not_take_over_ui_drags() {
    let (mut input, mut view, position) = setup();
    assert!(
        !input
            .mouse_button(true, Some(position), &mut view, true, egui::Modifiers::NONE)
            .0
    );
    input.mouse_pen = true;
    assert!(
        !input
            .mouse_button(
                true,
                Some(Point::new(10.0, 10.0)),
                &mut view,
                true,
                egui::Modifiers::NONE
            )
            .0
    );
    assert!(
        input
            .mouse_move(position, &mut view, egui::Modifiers::NONE)
            .is_none()
    );
    assert!(
        !input
            .mouse_button(
                false,
                Some(position),
                &mut view,
                true,
                egui::Modifiers::NONE
            )
            .0
    );
    assert!(
        input
            .mouse_button(true, Some(position), &mut view, true, egui::Modifiers::NONE)
            .0
    );
    assert!(
        input
            .mouse_move(
                Point::new(position.x + 5.0, position.y),
                &mut view,
                egui::Modifiers::NONE,
            )
            .is_none()
    );
    let stroke = input
        .mouse_button(
            false,
            Some(position),
            &mut view,
            true,
            egui::Modifiers::NONE,
        )
        .1
        .unwrap();
    assert_eq!(stroke.raw_samples.len(), 3);
    assert!(
        stroke
            .raw_samples
            .iter()
            .all(|s| s.pressure == Some(0.5) && s.native.is_none())
    );
    input.mouse_button(true, Some(position), &mut view, true, egui::Modifiers::NONE);
    input.cancel();
    assert!(
        input
            .mouse_button(
                false,
                Some(position),
                &mut view,
                true,
                egui::Modifiers::NONE
            )
            .1
            .is_none()
    );
}

#[test]
fn mouse_modifier_navigation_works_even_with_mouse_pen_off() {
    let (mut input, mut view, position) = setup();
    assert!(!input.mouse_pen);
    let zoom = view.transform.zoom();
    assert!(
        input
            .mouse_button(true, Some(position), &mut view, true, egui::Modifiers::CTRL)
            .0
    );
    let moved = Point::new(position.x, position.y - 60.0);
    assert!(
        input
            .mouse_move(moved, &mut view, egui::Modifiers::CTRL)
            .is_none()
    );
    assert!(view.transform.zoom() > zoom);
    assert!(
        input
            .mouse_button(false, Some(moved), &mut view, true, egui::Modifiers::CTRL)
            .1
            .is_none()
    );
    assert!(input.active().is_none());
    // A mouse click outside the canvas still belongs to its UI widget.
    assert!(
        !input
            .mouse_button(
                true,
                Some(Point::new(10.0, 10.0)),
                &mut view,
                true,
                egui::Modifiers::ALT
            )
            .0
    );
}

#[test]
fn mouse_tool_switch_commits_ink_from_keyboard_move_or_release_once() {
    for modifiers in [egui::Modifiers::CTRL, egui::Modifiers::ALT] {
        for trigger in 0..3 {
            let (mut input, mut view, position) = setup();
            input.mouse_pen = true;
            input.mouse_button(true, Some(position), &mut view, true, egui::Modifiers::NONE);
            let endpoint = Point::new(position.x + 10.0, position.y + 5.0);
            assert!(
                input
                    .mouse_move(endpoint, &mut view, egui::Modifiers::NONE)
                    .is_none()
            );
            let expected = input.active().unwrap().record;
            let completed = match trigger {
                0 => input.modifiers_changed(modifiers, &mut view),
                1 => input.mouse_move(
                    Point::new(endpoint.x, endpoint.y - 50.0),
                    &mut view,
                    modifiers,
                ),
                _ => {
                    input
                        .mouse_button(false, Some(endpoint), &mut view, true, modifiers)
                        .1
                }
            }
            .unwrap();
            assert_eq!(completed.as_ref(), expected.as_ref());
            assert_eq!(completed.raw_samples.len(), 2);
            assert!(input.modifiers_changed(modifiers, &mut view).is_none());
            assert!(
                input
                    .mouse_button(false, Some(endpoint), &mut view, true, modifiers)
                    .1
                    .is_none()
            );
            assert!(!input.is_drawing());
        }
    }
}
