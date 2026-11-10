use super::*;

#[test]
fn cross_page_batch_preserves_both_page_orders_snapshots_and_undo_redo() {
    use crate::document::Point;
    let objects = [
        stroke(0),
        stroke(0),
        stroke(0),
        stroke(1),
        stroke(1),
        stroke(2),
    ];
    let mut history = EditHistory::from_strokes(objects.iter().cloned());
    let ids = HashSet::from([objects[0].id, objects[2].id, objects[3].id]);
    assert!(history.move_strokes(
        &ids,
        InkTransform {
            translation: Point::new(50., -300.),
            ..Default::default()
        },
        1
    ));
    assert_eq!(history.stroke_count(), objects.len());
    assert_eq!(
        history.strokes(0).iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![objects[1].id]
    );
    assert_eq!(
        history.strokes(1).iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![objects[3].id, objects[4].id, objects[0].id, objects[2].id]
    );
    let moved = history.snapshot();
    let moved_revision = history.revision().current;
    history.mark_saved(moved_revision);
    assert!(!history.revision().is_dirty());
    for s in history.strokes(1).iter().filter(|s| ids.contains(&s.id)) {
        let before = objects.iter().find(|original| original.id == s.id).unwrap();
        assert_eq!(s.page_index, 1);
        assert_eq!(s.raw_samples, before.raw_samples);
        assert_ne!(s.modeled_samples, before.modeled_samples);
    }
    assert!(history.undo());
    assert!(!history.can_undo());
    for (a, b) in history.snapshot().iter().zip(&objects) {
        assert!(Arc::ptr_eq(a, b));
    }
    assert!(history.revision().is_dirty());
    assert!(history.redo());
    for (a, b) in history.snapshot().iter().zip(&moved) {
        assert!(Arc::ptr_eq(a, b));
    }
    assert!(!history.revision().is_dirty());
    assert!(history.remove_strokes(&ids));
    assert!(history.undo());
    assert_eq!(history.revision().current, moved_revision);
    assert!(history.undo());
    for (a, b) in history.snapshot().iter().zip(&objects) {
        assert!(Arc::ptr_eq(a, b));
    }
    assert!(history.move_strokes(&HashSet::from([objects[0].id]), InkTransform::default(), 3));
    assert!(!history.can_redo());
    assert_eq!(history.strokes(3)[0].id, objects[0].id);
    assert_eq!(
        history.strokes(3)[0].modeled_samples,
        objects[0].modeled_samples
    );
}

#[test]
fn transform_is_atomic_preserves_order_raw_capture_and_saved_snapshot_and_rebranches() {
    use crate::document::Point;
    let objects = [stroke(0), stroke(0), stroke(0), stroke(1)];
    let mut history = EditHistory::from_strokes(objects.iter().cloned());
    let ids = HashSet::from([objects[0].id, objects[2].id, objects[3].id]);
    let transform = InkTransform {
        center: Point::new(130., 100.),
        translation: Point::new(30., -20.),
        radians: std::f64::consts::FRAC_PI_2,
    };
    assert!(history.transform_strokes(&ids, transform));
    assert_eq!(history.stroke_count(), 4);
    let changed = history.snapshot();
    for (after, before) in changed.iter().zip(&objects) {
        assert_eq!(after.id, before.id);
        assert_eq!(after.raw_samples, before.raw_samples);
        assert_eq!(after.style, before.style);
        assert_eq!(after.processing, before.processing);
        assert_eq!(after.page_index, before.page_index);
        if ids.contains(&before.id) {
            assert_ne!(after.outline, before.outline);
            for (a, b) in after.modeled_samples.iter().zip(&before.modeled_samples) {
                assert_eq!(a.position, transform.apply(b.position));
                assert_eq!(a.time_us, b.time_us);
                assert_eq!(a.pressure, b.pressure);
            }
            for raw in &after.raw_samples {
                let a = after.edit_transform.apply(raw.position);
                let b = transform.apply(raw.position);
                assert!((a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9);
            }
        } else {
            assert!(Arc::ptr_eq(after, before));
        }
    }
    let transformed_revision = history.revision().current;
    history.mark_saved(transformed_revision);
    assert!(history.undo());
    assert!(history.revision().is_dirty());
    for (a, b) in history.snapshot().iter().zip(&objects) {
        assert!(Arc::ptr_eq(a, b));
    }
    assert!(!history.can_undo(), "one batch is one undo");
    assert!(history.redo());
    assert!(!history.revision().is_dirty());
    for (a, b) in history.snapshot().iter().zip(&changed) {
        assert!(Arc::ptr_eq(a, b));
    }
    history.undo();
    assert!(!history.transform_strokes(&ids, InkTransform::default()));
    assert!(history.can_redo(), "a tap must not erase redo");
    assert!(history.transform_strokes(
        &ids,
        InkTransform {
            translation: Point::new(-5., 12.),
            ..Default::default()
        }
    ));
    assert!(history.revision().current > transformed_revision);
    assert!(!history.can_redo());
    // In-flight save records still own the original geometry.
    assert_ne!(changed[0].outline, history.strokes(0)[0].outline);
}

#[test]
fn saved_objects_batch_delete_restores_original_order_and_clean_checkpoint() {
    let objects = [stroke(0), stroke(0), stroke(0), stroke(1), stroke(1)];
    let mut history = EditHistory::from_strokes(objects.iter().cloned());
    assert!(!history.can_undo() && !history.revision().is_dirty());
    let ids = HashSet::from([objects[0].id, objects[2].id, objects[3].id]);
    assert!(history.remove_strokes(&ids));
    let revision = history.revision();
    assert!(!history.remove_strokes(&ids));
    assert_eq!(history.revision(), revision);
    assert_eq!(history.strokes(0)[0].id, objects[1].id);
    assert!(history.undo());
    assert!(!history.revision().is_dirty());
    for (restored, original) in history.snapshot().iter().zip(objects.iter()) {
        assert!(Arc::ptr_eq(restored, original));
    }
    assert!(history.redo());
    history.mark_saved(revision.current);
    assert!(!history.revision().is_dirty());
    assert!(history.undo());
    assert!(history.revision().is_dirty());
    history.execute(EditCommand::AddStroke(stroke(0)));
    assert!(!history.can_redo());
}

fn stroke(page: u32) -> Arc<StrokeRecord> {
    use crate::document::{PageGeometry, Rect, Rotation};
    let geometry = PageGeometry::new(Rect::new(0.0, 0.0, 600.0, 800.0), Rotation::R0, 1.0).unwrap();
    let mut stroke = crate::ink::test_support::pressure_stroke(geometry).unwrap();
    stroke.page_index = page;
    Arc::new(stroke)
}

#[test]
fn global_history_restores_page_order_and_the_original_records() {
    let mut edits = EditHistory::default();
    let a = stroke(1);
    let b = stroke(0);
    let c = stroke(1);
    for stroke in [&a, &b, &c] {
        assert!(edits.execute(EditCommand::AddStroke(Arc::clone(stroke))));
    }
    assert_eq!(edits.stroke_count(), 3);
    assert_eq!(
        edits.strokes(1).iter().map(|s| s.id).collect::<Vec<_>>(),
        [a.id, c.id]
    );
    assert!(edits.undo());
    assert!(edits.undo());
    assert!(edits.strokes(0).is_empty());
    assert_eq!(edits.stroke_count(), 1);
    assert!(edits.redo());
    assert!(edits.redo());
    assert!(Arc::ptr_eq(&edits.strokes(0)[0], &b));
    assert!(Arc::ptr_eq(&edits.strokes(1)[0], &a));
    assert!(Arc::ptr_eq(&edits.strokes(1)[1], &c));
    assert_eq!(*edits.strokes(1)[1], *c);
    assert!(!edits.redo());
}

#[test]
fn undo_to_initial_state_is_clean_and_empty_history_does_not_change_revision() {
    let mut edits = EditHistory::default();
    assert!(!edits.revision().is_dirty());
    assert!(!edits.undo());
    assert!(!edits.redo());
    edits.execute(EditCommand::AddStroke(stroke(0)));
    let added = edits.revision();
    assert!(added.is_dirty());
    assert!(edits.undo());
    assert_eq!(edits.stroke_count(), 0);
    assert!(!edits.revision().is_dirty());
    assert!(!edits.undo());
    assert!(edits.redo());
    assert_eq!(edits.revision(), added);
}

#[test]
fn saved_session_strokes_remain_undoable_in_both_directions() {
    let mut edits = EditHistory::default();
    edits.execute(EditCommand::AddStroke(stroke(0)));
    let saved = edits.revision().current;
    edits.mark_saved(saved);
    assert!(!edits.revision().is_dirty());
    edits.execute(EditCommand::AddStroke(stroke(1)));
    assert!(edits.revision().is_dirty());
    edits.undo();
    assert!(!edits.revision().is_dirty());
    edits.undo();
    assert_eq!(edits.stroke_count(), 0);
    assert!(edits.revision().is_dirty());
    edits.redo();
    assert!(!edits.revision().is_dirty());
    edits.redo();
    assert!(edits.revision().is_dirty());
}

#[test]
fn branch_at_saved_depth_gets_a_new_identity_and_cannot_redo_abandoned_ink() {
    let mut edits = EditHistory::default();
    edits.execute(EditCommand::AddStroke(stroke(0)));
    edits.execute(EditCommand::AddStroke(stroke(0)));
    let saved = edits.revision().current;
    edits.mark_saved(saved);
    edits.undo();
    let replacement = stroke(1);
    edits.execute(EditCommand::AddStroke(Arc::clone(&replacement)));
    assert_eq!(edits.stroke_count(), 2);
    assert!(edits.revision().current > saved);
    assert!(edits.revision().is_dirty());
    assert!(!edits.can_redo());
    assert!(!edits.redo());
    edits.undo();
    edits.redo();
    assert!(Arc::ptr_eq(&edits.strokes(1)[0], &replacement));
    assert!(edits.revision().is_dirty());
}

#[test]
fn delayed_save_marks_only_the_captured_revision() {
    let mut edits = EditHistory::default();
    edits.execute(EditCommand::AddStroke(stroke(0)));
    let saving = edits.revision().current;
    edits.execute(EditCommand::AddStroke(stroke(1)));
    edits.mark_saved(saving);
    assert!(edits.revision().is_dirty());
    assert!(edits.can_undo());
    edits.undo();
    assert!(!edits.revision().is_dirty());
    edits.undo();
    edits.execute(EditCommand::AddStroke(stroke(0)));
    assert!(edits.revision().is_dirty());
}

#[test]
fn duplicate_submission_preserves_ink_revision_and_redo() {
    let mut edits = EditHistory::default();
    let a = stroke(0);
    edits.execute(EditCommand::AddStroke(Arc::clone(&a)));
    edits.execute(EditCommand::AddStroke(stroke(1)));
    edits.undo();
    let before = edits.revision();
    assert!(!edits.execute(EditCommand::AddStroke(a)));
    assert_eq!(edits.revision(), before);
    assert_eq!(edits.stroke_count(), 1);
    assert!(edits.can_redo());
    assert!(edits.redo());
    assert_eq!(edits.stroke_count(), 2);
}
