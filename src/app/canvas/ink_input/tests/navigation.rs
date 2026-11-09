use super::*;
use crate::app::canvas::view::ViewAction;

#[test]
fn ctrl_pen_zoom_uses_logical_distance_and_keeps_its_anchor_at_each_dpi() {
    for dpi in [1.0, 1.5, 2.0] {
        let (mut input, mut view, _) = setup();
        let old = view.viewport.unwrap();
        let viewport = CanvasViewport::new(
            old.logical,
            dpi,
            ((800.0 * dpi) as u32, (640.0 * dpi) as u32),
        )
        .unwrap();
        view.update_viewport(viewport);
        let anchor = Point::new(430.0, 300.0);
        let position = viewport.logical_to_physical(anchor);
        let before = view.pointer_coordinates(anchor).unwrap();
        let zoom = view.transform.zoom();
        // Ctrl takes priority if Alt is held too.
        let modifiers = egui::Modifiers {
            ctrl: true,
            alt: true,
            ..egui::Modifiers::NONE
        };
        input.pen(sample(PenPhase::Down, position), &mut view, true, modifiers);
        let moved = Point::new(position.x + 20.0 * dpi, position.y - 100.0 * dpi);
        input.pen(sample(PenPhase::Move, moved), &mut view, true, modifiers);
        assert!((view.transform.zoom() / zoom - 0.5_f64.exp()).abs() < 1e-8);
        let after = view.pointer_coordinates(anchor).unwrap();
        assert_eq!(before.0, after.0);
        assert!((before.2.x - after.2.x).abs() < 1e-8 && (before.2.y - after.2.y).abs() < 1e-8);
        assert!(
            input
                .pen(sample(PenPhase::Up, moved), &mut view, true, modifiers)
                .1
                .is_none()
        );
        assert!(input.active().is_none());
    }
}

#[test]
fn alt_pen_drag_pans_without_ink_and_ignores_other_contacts() {
    let (mut input, mut view, position) = setup();
    let viewport = view.viewport.unwrap();
    view.apply_action(ViewAction::Zoom(4.0), viewport.logical);
    let origin = view.transform.paper_to_logical(Point::default());
    input.pen(
        sample(PenPhase::Down, position),
        &mut view,
        true,
        egui::Modifiers::ALT,
    );
    let moved = Point::new(position.x + 30.0, position.y + 45.0);
    let mut other = sample(PenPhase::Move, moved);
    other.pointer_id += 1;
    input.pen(other, &mut view, true, egui::Modifiers::ALT);
    assert_eq!(origin, view.transform.paper_to_logical(Point::default()));
    input.pen(
        sample(PenPhase::Move, moved),
        &mut view,
        true,
        egui::Modifiers::ALT,
    );
    let after = view.transform.paper_to_logical(Point::default());
    assert!((after.x - origin.x - 20.0).abs() < 1e-8 && (after.y - origin.y - 30.0).abs() < 1e-8);
    assert!(
        input
            .pen(
                sample(PenPhase::Up, moved),
                &mut view,
                true,
                egui::Modifiers::ALT
            )
            .1
            .is_none()
    );
}

#[test]
fn modifier_navigation_keeps_ui_ownership_and_cancels_cleanly() {
    let (mut input, mut view, position) = setup();
    let before = view.transform;
    let toolbar = Point::new(10.0, 10.0);
    assert!(
        input
            .pen(
                sample(PenPhase::Down, toolbar),
                &mut view,
                true,
                egui::Modifiers::CTRL
            )
            .0
    );
    assert!(
        input
            .pen(
                sample(PenPhase::Move, position),
                &mut view,
                true,
                egui::Modifiers::ALT
            )
            .0
    );
    assert!(
        input
            .pen(
                sample(PenPhase::Up, position),
                &mut view,
                true,
                egui::Modifiers::ALT
            )
            .0
    );
    assert_eq!(view.transform, before);
    input.pen(
        sample(PenPhase::Down, position),
        &mut view,
        true,
        egui::Modifiers::CTRL,
    );
    input.cancel();
    input.pen(
        sample(PenPhase::Move, toolbar),
        &mut view,
        true,
        egui::Modifiers::CTRL,
    );
    assert_eq!(view.transform, before);
    assert!(
        input
            .pen(
                sample(PenPhase::Up, position),
                &mut view,
                true,
                egui::Modifiers::CTRL
            )
            .1
            .is_none()
    );
}
