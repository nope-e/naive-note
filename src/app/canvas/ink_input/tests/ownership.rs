use super::*;

#[test]
fn down_in_ui_cannot_become_canvas_ink_and_cancel_is_terminal() {
    let (mut input, mut view, position) = setup();
    let toolbar = Point::new(10.0, 10.0);
    assert!(
        input
            .pen(
                sample(PenPhase::Down, toolbar),
                &mut view,
                true,
                egui::Modifiers::NONE
            )
            .0
    );
    assert!(
        input
            .pen(
                sample(PenPhase::Move, position),
                &mut view,
                true,
                egui::Modifiers::NONE
            )
            .0
    );
    input.cancel_drawing();
    let (ui, stroke) = input.pen(
        sample(PenPhase::Up, position),
        &mut view,
        true,
        egui::Modifiers::NONE,
    );
    assert!(ui && stroke.is_none());
    input.pen(
        sample(PenPhase::Down, position),
        &mut view,
        true,
        egui::Modifiers::NONE,
    );
    assert!(input.is_drawing());
    input.pen(
        sample(PenPhase::Cancel, position),
        &mut view,
        true,
        egui::Modifiers::NONE,
    );
    assert!(!input.is_drawing());
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
}

#[test]
fn outside_page_or_loading_contacts_never_join_a_stroke_midway() {
    let (mut input, mut view, position) = setup();
    let target = view.stroke_target(position).unwrap();
    let gap = target
        .viewport
        .logical_to_physical(target.view.paper_to_logical(Point::new(
            150.0,
            target.original.paper_size().height + 12.0,
        )));
    assert!(
        target
            .viewport
            .logical
            .contains(target.viewport.physical_to_logical(gap))
    );
    assert!(view.stroke_target(gap).is_none());
    for (start, enabled) in [
        (Point::new(1.0, 400.0), true),
        (gap, true),
        (position, false),
    ] {
        input.pen(
            sample(PenPhase::Down, start),
            &mut view,
            enabled,
            egui::Modifiers::NONE,
        );
        input.pen(
            sample(PenPhase::Move, position),
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
                .is_none()
        );
    }
    for phase in [
        PenPhase::Hover,
        PenPhase::Move,
        PenPhase::Up,
        PenPhase::Cancel,
    ] {
        assert!(
            input
                .pen(
                    sample(phase, position),
                    &mut view,
                    true,
                    egui::Modifiers::NONE
                )
                .1
                .is_none()
        );
        assert!(!input.is_drawing());
    }
}

#[test]
fn another_pointer_cannot_move_finish_or_cancel_the_owned_contact() {
    let (mut input, mut view, position) = setup();
    input.pen(
        sample(PenPhase::Down, position),
        &mut view,
        true,
        egui::Modifiers::NONE,
    );
    for phase in [
        PenPhase::Down,
        PenPhase::Move,
        PenPhase::Up,
        PenPhase::Cancel,
    ] {
        let mut other = sample(phase, position);
        other.pointer_id += 1;
        other.stroke_id = Some(8);
        assert!(
            input
                .pen(other, &mut view, true, egui::Modifiers::NONE)
                .1
                .is_none()
        );
    }
    let stroke = input
        .pen(
            sample(PenPhase::Up, position),
            &mut view,
            true,
            egui::Modifiers::NONE,
        )
        .1
        .unwrap();
    assert_eq!(stroke.raw_samples.len(), 2);
}
