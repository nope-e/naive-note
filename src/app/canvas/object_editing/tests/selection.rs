//! Selection/history state only; no UI layout or rendering.

use super::*;

#[test]
fn full_turn_is_a_noop_and_rotation_crosses_angle_boundary_without_reversing() {
    use crate::{document::InkTransform, render::CanvasScene};
    let (mut document, target) = fixture();
    let a = stroke(target, 100.);
    document.add_stroke(a.clone());
    let revision = document.revision();
    let mut editor = ObjectEditing::default();
    editor.select_page(&document, 0);
    let mut view = super::super::super::view::CanvasView::new(vec![target.original]);
    view.update_viewport(target.viewport);
    let mut scene = CanvasScene {
        viewport: target.viewport,
        pages: view.scenes(target.viewport, true),
    };
    scene.pages[0].strokes = document.strokes(0).to_vec();
    editor.decorate(&mut scene);
    let handle = scene.pages[0]
        .selection_overlay
        .as_ref()
        .unwrap()
        .rotation_handle;
    let center = a.bounds().center();
    for angles in [
        &[0., 90., 180., 270., 360.][..],
        &[0., 150., 210., 330., 390.][..],
    ] {
        let mut previous = handle;
        for &degrees in angles {
            let transform = InkTransform {
                center,
                radians: degrees * std::f64::consts::PI / 180.,
                ..Default::default()
            };
            let position = transform.apply(handle);
            editor.receive(
                vec![ObjectEvent::Update(ObjectGesture {
                    target,
                    destination: Some(target),
                    tool: CanvasTool::Lasso,
                    start: handle,
                    previous,
                    position,
                    additive: false,
                })],
                &mut document,
            );
            previous = position;
        }
        editor.receive(vec![ObjectEvent::Finish], &mut document);
        if angles.last() == Some(&360.) {
            assert_eq!(document.revision(), revision);
            assert!(Arc::ptr_eq(&document.strokes(0)[0], &a));
        }
    }
    let expected = InkTransform {
        center,
        radians: std::f64::consts::PI / 6.,
        ..Default::default()
    }
    .apply(a.modeled_samples[0].position);
    let actual = document.strokes(0)[0].modeled_samples[0].position;
    assert!((actual.x - expected.x).abs() < 1e-9 && (actual.y - expected.y).abs() < 1e-9);
    assert_eq!(document.revision().current, revision.current + 1);
}

fn lasso(target: StrokeTarget, top: f64, bottom: f64, additive: bool) -> Vec<ObjectEvent> {
    let start = Point::new(70., top);
    let mut previous = start;
    let mut events = Vec::new();
    for position in [
        start,
        Point::new(190., top),
        Point::new(190., bottom),
        Point::new(70., bottom),
        start,
    ] {
        events.push(ObjectEvent::Update(ObjectGesture {
            target,
            destination: Some(target),
            tool: CanvasTool::Lasso,
            start,
            previous,
            position,
            additive,
        }));
        previous = position;
    }
    events.push(ObjectEvent::Finish);
    events
}

#[test]
fn closed_lasso_additive_partial_and_cancel_are_page_bound_and_do_not_dirty_document() {
    let (mut document, target) = fixture();
    let a = stroke(target, 100.);
    let b = stroke(target, 200.);
    let mut other = (*a).clone();
    other.id = Uuid::new_v4();
    other.page_index = 1;
    for record in [a.clone(), b.clone(), Arc::new(other)] {
        document.add_stroke(record);
    }
    let revision = document.revision();
    let mut editor = ObjectEditing::default();
    editor.receive(lasso(target, 80., 120., false), &mut document);
    assert_eq!(editor.selected, HashSet::from([a.id]));
    editor.receive(lasso(target, 180., 220., true), &mut document);
    assert_eq!(editor.selected, HashSet::from([a.id, b.id]));
    let mut canceled = lasso(target, 180., 220., false);
    *canceled.last_mut().unwrap() = ObjectEvent::Cancel;
    editor.receive(canceled, &mut document);
    assert_eq!(editor.selected, HashSet::from([a.id, b.id]));
    editor.clear();
    // The pressure outline protrudes past both edges of this narrow lasso.
    editor.receive(lasso(target, 99., 101., false), &mut document);
    assert!(editor.selected.is_empty());
    assert_eq!(document.revision(), revision);
}

#[test]
fn move_preview_cancel_and_noop_leave_history_intact_and_finish_is_one_edit() {
    let (mut document, target) = fixture();
    let a = stroke(target, 100.);
    let b = stroke(target, 200.);
    for record in [&a, &b] {
        document.add_stroke(record.clone());
    }
    let revision = document.revision();
    let mut editor = ObjectEditing::default();
    editor.select_page(&document, 0);
    let start = Point::new(130., 150.);
    let moving = gesture(
        target,
        CanvasTool::Lasso,
        start,
        Point::new(160., 190.),
        false,
    );
    editor.receive(vec![ObjectEvent::Update(moving)], &mut document);
    assert_eq!(document.revision(), revision);
    assert!(Arc::ptr_eq(&document.strokes(0)[0], &a));
    assert_eq!(editor.selected_count(), 2);
    editor.receive(vec![ObjectEvent::Cancel], &mut document);
    assert_eq!(editor.selected, HashSet::from([a.id, b.id]));
    assert_eq!(document.revision(), revision);
    editor.receive(
        vec![ObjectEvent::Update(moving), ObjectEvent::Finish],
        &mut document,
    );
    let changed = document.revision();
    assert_eq!(changed.current, revision.current + 1);
    for (moved, original) in document.strokes(0).iter().zip([&a, &b]) {
        assert_eq!(moved.id, original.id);
        assert_eq!(moved.raw_samples, original.raw_samples);
        let expected = Point::new(
            original.modeled_samples[0].position.x + 30.,
            original.modeled_samples[0].position.y + 40.,
        );
        assert_eq!(moved.modeled_samples[0].position, expected);
    }
    document.undo();
    assert_eq!(document.revision(), revision);
    assert!(Arc::ptr_eq(&document.strokes(0)[0], &a));
    let tap = gesture(target, CanvasTool::Select, start, start, false);
    editor.receive(
        vec![ObjectEvent::Update(tap), ObjectEvent::Finish],
        &mut document,
    );
    assert_eq!(document.revision(), revision);
    assert!(document.can_redo());
    document.redo();
    assert_eq!(document.revision(), changed);
    editor.delete_selected(&mut document);
    assert!(document.strokes(0).is_empty());
    document.undo();
    assert_eq!(document.revision(), changed);
}
