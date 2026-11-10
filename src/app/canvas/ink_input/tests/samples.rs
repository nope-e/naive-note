use super::*;
use crate::app::canvas::view::ViewAction;

#[test]
fn dot_keeps_raw_attributes_equal_timestamps_and_missing_pressure() {
    let (mut input, mut view, position) = setup();
    assert!(
        !input
            .pen(
                sample(PenPhase::Down, position),
                &mut view,
                true,
                egui::Modifiers::NONE
            )
            .0
    );
    let mut next = sample(PenPhase::Move, position);
    next.pressure = Some(0.0);
    input.pen(next, &mut view, true, egui::Modifiers::NONE);
    let (_, stroke) = input.pen(
        sample(PenPhase::Up, position),
        &mut view,
        true,
        egui::Modifiers::NONE,
    );
    let stroke = stroke.unwrap();
    assert_eq!(stroke.raw_samples.len(), 3);
    assert_eq!(stroke.raw_samples[0].pressure, None);
    assert_eq!(stroke.raw_samples[1].pressure, Some(0.0));
    assert_eq!(stroke.modeled_samples[0].pressure, 0.5);
    assert_eq!(stroke.modeled_samples[1].pressure, 0.0);
    assert!(stroke.raw_samples.iter().all(|s| s.time_us == 123_456));
    let raw = &stroke.raw_samples[0];
    assert_eq!(raw.tilt_degrees, None);
    let native = raw.native.as_ref().unwrap();
    assert_eq!(native.tilt_x_degrees, Some(-30.0));
    assert_eq!(native.tilt_y_degrees, None);
    assert_eq!(native.client_position_px, [position.x, position.y]);
    assert_eq!(native.pressure_level, Some(16_384));
    assert!(native.barrel_button);
    assert!(!stroke.outline.is_empty());
    let mut bytes = Vec::new();
    ciborium::into_writer(stroke.as_ref(), &mut bytes).unwrap();
    let restored: StrokeRecord = ciborium::from_reader(bytes.as_slice()).unwrap();
    assert_eq!(*stroke, restored);
    assert!(!input.is_drawing());
}

#[test]
fn pen_on_a_rotated_second_page_keeps_its_own_page_and_paper_coordinates() {
    let (mut input, mut view, position) = setup();
    let viewport = view.viewport.unwrap();
    view.apply_action(ViewAction::NextPage, viewport.logical);
    let target = view.stroke_target(position).unwrap();
    assert_eq!(target.page_index, 1);
    input.pen(
        sample(PenPhase::Down, position),
        &mut view,
        true,
        egui::Modifiers::NONE,
    );
    let stroke = input
        .pen(
            sample(PenPhase::Up, position),
            &mut view,
            true,
            egui::Modifiers::NONE,
        )
        .1
        .unwrap();
    assert_eq!(stroke.page_index, 1);
    assert_eq!(
        stroke.raw_samples[0].position,
        target.paper_position(position)
    );
    assert!(
        target
            .original
            .contains_paper(stroke.raw_samples[0].position)
    );
}

#[test]
fn page_and_down_transform_remain_bound_outside_the_page() {
    let (mut input, mut view, position) = setup();
    let target = view.stroke_target(position).unwrap();
    input.pen(
        sample(PenPhase::Down, position),
        &mut view,
        true,
        egui::Modifiers::NONE,
    );
    let snapshot = input.active().unwrap().record;
    // Simulate a later navigation change: mapping must still use Down's view.
    view.transform.pan(Point::new(200.0, -900.0));
    let outside = Point::new(position.x + 900.0, position.y + 1500.0);
    input.pen(
        sample(PenPhase::Move, outside),
        &mut view,
        true,
        egui::Modifiers::NONE,
    );
    let stroke = input
        .pen(
            sample(PenPhase::Up, outside),
            &mut view,
            true,
            egui::Modifiers::NONE,
        )
        .1
        .unwrap();
    assert_eq!(stroke.page_index, 0);
    assert_eq!(
        stroke.raw_samples[1].position,
        target.paper_position(outside)
    );
    assert!(
        !target
            .original
            .contains_paper(stroke.raw_samples[1].position)
    );
    assert_eq!(
        snapshot.raw_samples.len(),
        1,
        "published snapshots are immutable"
    );
    assert_eq!(stroke.raw_samples.len(), 3);
    assert!(
        input
            .pen(
                sample(PenPhase::Up, outside),
                &mut view,
                true,
                egui::Modifiers::NONE
            )
            .1
            .is_none()
    );
}

#[test]
fn eraser_end_routes_an_object_gesture_without_ink() {
    let (mut input, mut view, position) = setup();
    let mut down = sample(PenPhase::Down, position);
    down.eraser = true;
    input.pen(down, &mut view, true, egui::Modifiers::NONE);
    assert!(input.active().is_none());
    assert!(input.is_drawing());
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
    assert!(matches!(
        input.take_object_events().last(),
        Some(super::super::ObjectEvent::Finish)
    ));
}
