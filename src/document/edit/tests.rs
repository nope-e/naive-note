use super::*;

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
