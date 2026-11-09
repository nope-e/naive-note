use super::*;
use crate::app::canvas::view::ViewAction;

#[test]
fn changing_modifiers_mid_stroke_finishes_ink_once_and_never_rejoins_after_release() {
    let (mut input, mut view, position) = setup();
    input.pen(
        sample(PenPhase::Down, position),
        &mut view,
        true,
        egui::Modifiers::NONE,
    );
    assert!(input.is_drawing());
    let expected = input.active().unwrap();
    let before = view.transform;
    let completed = input
        .modifiers_changed(egui::Modifiers::CTRL, &mut view)
        .unwrap();
    assert_eq!(
        completed.as_ref(),
        expected.as_ref(),
        "keep all ink and its original endpoint"
    );
    assert!(
        input
            .modifiers_changed(egui::Modifiers::CTRL, &mut view)
            .is_none()
    );
    assert!(
        !input.is_drawing(),
        "a key press must take effect before the next pen sample"
    );
    assert!(
        input
            .pen(
                sample(PenPhase::Move, position),
                &mut view,
                true,
                egui::Modifiers::CTRL,
            )
            .1
            .is_none()
    );
    assert!(!input.is_drawing());
    assert_eq!(before, view.transform, "tool switches cannot jump the view");
    let moved = Point::new(position.x, position.y - 60.0);
    input.pen(
        sample(PenPhase::Move, moved),
        &mut view,
        true,
        egui::Modifiers::CTRL,
    );
    assert!(view.transform.zoom() > before.zoom());
    let paused = view.transform;
    input.pen(
        sample(PenPhase::Move, position),
        &mut view,
        true,
        egui::Modifiers::NONE,
    );
    assert_eq!(paused, view.transform);
    assert!(
        input
            .pen(
                sample(PenPhase::Up, position),
                &mut view,
                true,
                egui::Modifiers::NONE
            )
            .1
            .is_none()
    );
    input.pen(
        sample(PenPhase::Down, position),
        &mut view,
        true,
        egui::Modifiers::NONE,
    );
    assert!(
        input
            .pen(
                sample(PenPhase::Up, position),
                &mut view,
                true,
                egui::Modifiers::NONE
            )
            .1
            .is_some()
    );
    assert_eq!(
        completed.as_ref(),
        expected.as_ref(),
        "navigation cannot alter committed ink"
    );
}

#[test]
fn pen_sample_tool_switch_finishes_existing_ink_for_both_move_and_up() {
    for modifiers in [egui::Modifiers::CTRL, egui::Modifiers::ALT] {
        for phase in [PenPhase::Move, PenPhase::Up] {
            let (mut input, mut view, position) = setup();
            let viewport = view.viewport.unwrap();
            view.apply_action(ViewAction::Zoom(4.0), viewport.logical);
            input.pen(
                sample(PenPhase::Down, position),
                &mut view,
                true,
                egui::Modifiers::NONE,
            );
            let endpoint = Point::new(position.x + 10.0, position.y + 5.0);
            input.pen(
                sample(PenPhase::Move, endpoint),
                &mut view,
                true,
                egui::Modifiers::NONE,
            );
            let expected = input.active().unwrap();
            let before = view.transform;
            // The key snapshot can arrive with a sample before ModifiersChanged.
            let navigation_point = Point::new(endpoint.x + 50.0, endpoint.y - 60.0);
            let completed = input
                .pen(sample(phase, navigation_point), &mut view, true, modifiers)
                .1
                .unwrap();
            assert_eq!(completed.as_ref(), expected.as_ref());
            assert_eq!(completed.raw_samples.len(), 2);
            assert_eq!(
                view.transform, before,
                "start navigation without jumping or extending ink"
            );
            assert!(input.modifiers_changed(modifiers, &mut view).is_none());
            if phase == PenPhase::Move {
                input.pen(sample(PenPhase::Move, endpoint), &mut view, true, modifiers);
                assert_ne!(view.transform, before);
            }
            assert!(
                input
                    .pen(sample(PenPhase::Up, endpoint), &mut view, true, modifiers)
                    .1
                    .is_none()
            );
            input.cancel();
            assert_eq!(
                completed.as_ref(),
                expected.as_ref(),
                "later cancellation preserves finished ink"
            );
        }
    }
}
