use super::*;

#[test]
fn modeling_defaults_on_and_mode_changes_apply_only_to_next_contact() {
    assert!(CanvasInput::default().smoothing);
    for initial in [true, false] {
        let (mut input, mut view, position) = setup();
        input.smoothing = initial;
        input.pen(
            sample(PenPhase::Down, position),
            &mut view,
            true,
            egui::Modifiers::NONE,
        );
        input.smoothing = !initial;
        let mut up = sample(PenPhase::Up, Point::new(position.x + 30.0, position.y));
        up.time_us += 16_000;
        let completed = input
            .pen(up, &mut view, true, egui::Modifiers::NONE)
            .1
            .unwrap();
        assert_eq!(completed.processing.modeler.is_some(), initial);
        input.pen(
            sample(PenPhase::Down, position),
            &mut view,
            true,
            egui::Modifiers::NONE,
        );
        let completed = input
            .pen(up, &mut view, true, egui::Modifiers::NONE)
            .1
            .unwrap();
        assert_eq!(completed.processing.modeler.is_some(), !initial);
    }
}

#[test]
fn modeler_keeps_native_attributes_and_ctrl_finishes_before_navigation() {
    let (mut input, mut view, position) = setup();
    input.smoothing = true;
    let down = sample(PenPhase::Down, position);
    input.pen(down, &mut view, true, egui::Modifiers::NONE);
    let mut moved = sample(PenPhase::Move, Point::new(position.x + 40.0, position.y));
    moved.time_us += 16_000;
    input.pen(moved, &mut view, true, egui::Modifiers::NONE);
    let preview = input.active().unwrap();
    assert!(!preview.predicted_outline.is_empty());
    let completed = input
        .modifiers_changed(egui::Modifiers::CTRL, &mut view)
        .unwrap();
    assert_eq!(completed.raw_samples, preview.record.raw_samples);
    let native = completed.raw_samples[0].native.as_ref().unwrap();
    assert_eq!(native.tilt_x_degrees, Some(-30.0));
    assert_eq!(native.tilt_y_degrees, None);
    assert_eq!(completed.raw_samples[0].rotation_degrees, Some(15.0));
    assert!(completed.processing.modeler.is_some());
    assert!(input.active().is_none());
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

#[test]
fn cancellation_discards_model_and_prediction_before_next_down() {
    let (mut input, mut view, position) = setup();
    input.smoothing = true;
    for phase in [PenPhase::Down, PenPhase::Move] {
        let mut next = sample(phase, Point::new(position.x + 30.0, position.y));
        if phase == PenPhase::Move {
            next.time_us += 16_000
        }
        input.pen(next, &mut view, true, egui::Modifiers::NONE);
    }
    input.pen(
        sample(PenPhase::Cancel, position),
        &mut view,
        true,
        egui::Modifiers::NONE,
    );
    assert!(input.active().is_none());
    input.pen(
        sample(PenPhase::Down, position),
        &mut view,
        true,
        egui::Modifiers::NONE,
    );
    let completed = input
        .pen(
            sample(PenPhase::Up, position),
            &mut view,
            true,
            egui::Modifiers::NONE,
        )
        .1
        .unwrap();
    assert_eq!(completed.raw_samples.len(), 2);
    assert_eq!(
        completed.modeled_samples[0].position,
        completed.raw_samples[0].position
    );
    assert!(completed.processing.modeler.is_some());
}

#[test]
fn mouse_uses_selected_model_with_constant_pressure_in_both_modes() {
    for smoothing in [true, false] {
        let (mut input, mut view, position) = setup();
        input.smoothing = smoothing;
        input.mouse_pen = true;
        input.mouse_button(true, Some(position), &mut view, true, egui::Modifiers::NONE);
        input.mouse_move(
            Point::new(position.x + 30.0, position.y),
            &mut view,
            egui::Modifiers::NONE,
        );
        let completed = input
            .mouse_button(false, None, &mut view, true, egui::Modifiers::NONE)
            .1
            .unwrap();
        assert_eq!(completed.processing.modeler.is_some(), smoothing);
        assert_eq!(completed.raw_samples.len(), 2);
        assert!(
            completed
                .modeled_samples
                .iter()
                .all(|s| (s.pressure - 0.5).abs() < 1e-9)
        );
    }
}
