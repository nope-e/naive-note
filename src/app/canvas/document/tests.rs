//! Document/history lifecycle only; no event loop, UI layout or renderer.

use super::*;
use crate::document::{Rect, Rotation};

fn document() -> CanvasDocument {
    CanvasDocument::with_worker(PdfWorker::new(|_| {}).unwrap())
}

fn fixture_pdf() -> LoadedPdf {
    use crate::pdf::PageBitmap;
    let geometry = PageGeometry::new(Rect::new(0.0, 0.0, 600.0, 800.0), Rotation::R0, 1.0).unwrap();
    LoadedPdf {
        path: "original.pdf".into(),
        page_count: 1,
        pages: vec![geometry],
        first_page: Arc::new(PageBitmap {
            width: 1,
            height: 1,
            rgba: Arc::from([255; 4]),
            geometry,
            bounds: Rect::new(0.0, 0.0, 600.0, 800.0),
        }),
        edit_data_status: "Existing annotations in immutable base".to_owned(),
        fingerprint: crate::storage::FileFingerprint::of(b"fixture"),
    }
}

fn add(document: &mut CanvasDocument) {
    let stroke = crate::ink::test_support::pressure_stroke(fixture_pdf().pages[0]).unwrap();
    document.add_stroke(Arc::new(stroke));
}

#[test]
fn failed_open_preserves_dirty_history_and_stale_success_is_ignored() {
    let mut document = document();
    add(&mut document);
    add(&mut document);
    document.undo();
    let revision = document.revision();
    document.pending = Some(42);
    assert!(!document.needs_discard_confirmation());
    assert!(!document.can_undo());
    assert!(!document.can_redo());
    assert!(
        document
            .opened(OpenResponse {
                request: 41,
                result: Ok(fixture_pdf())
            })
            .is_none()
    );
    assert_eq!(document.pending, Some(42));
    assert_eq!(document.revision(), revision);
    assert!(
        document
            .opened(OpenResponse {
                request: 42,
                result: Err("invalid PDF".into())
            })
            .is_none()
    );
    assert_eq!(document.revision(), revision);
    assert_eq!(document.stroke_count(), 1);
    assert!(document.needs_discard_confirmation());
    assert!(document.can_undo() && document.can_redo());
    document.redo();
    assert_eq!(document.stroke_count(), 2);
    assert!(document.is_dirty());
}

#[test]
fn successful_open_and_close_start_fresh_histories_over_the_base() {
    let mut document = document();
    add(&mut document);
    document.pending = Some(1);
    assert!(
        document
            .opened(OpenResponse {
                request: 1,
                result: Ok(fixture_pdf())
            })
            .is_some()
    );
    assert!(document.is_loaded());
    assert!(!document.is_dirty());
    assert_eq!(document.stroke_count(), 0);
    assert!(!document.can_undo() && !document.can_redo());
    add(&mut document);
    document.close();
    assert!(!document.is_loaded());
    assert!(!document.is_dirty());
    assert_eq!(document.stroke_count(), 0);
    assert!(!document.can_undo() && !document.can_redo());
    assert!(!document.can_close());
    assert!(
        document
            .opened(OpenResponse {
                request: 1,
                result: Ok(fixture_pdf())
            })
            .is_none()
    );
}

#[test]
fn draft_can_close_and_readouts_follow_revision_including_saved_deletions() {
    let mut document = document();
    add(&mut document);
    assert!(document.can_close());
    assert!(document.subtitle().contains("未保存更改"));
    let saved = document.revision().current;
    document.edits.mark_saved(saved);
    document.undo();
    assert_eq!(document.stroke_count(), 0);
    assert!(document.subtitle().contains("未保存更改"));
    assert!(document.subtitle().contains("0 笔"));
    assert!(document.label(0).contains("Unsaved changes"));
    document.redo();
    assert!(!document.is_dirty());
    assert!(!document.subtitle().contains("未保存"));
    assert!(document.can_undo());
}

#[test]
fn save_completion_marks_only_its_snapshot_and_keeps_the_original_history() {
    use crate::pdf::{SaveResponse, SavedPdf};
    let mut document = document();
    add(&mut document);
    let saved_revision = document.revision().current;
    let original = Arc::clone(&document.edits.strokes(0)[0]);
    document.pending_save = Some((100, saved_revision));
    assert!(!document.can_save());
    assert!(document.can_ink() && document.can_undo());
    add(&mut document);
    document.saved(SaveResponse {
        document: None,
        request: 100,
        revision: saved_revision,
        result: Ok(SavedPdf {
            path: "saved.pdf".into(),
            fingerprint: crate::storage::FileFingerprint::of(b"saved"),
        }),
    });
    assert_eq!(document.revision().saved, saved_revision);
    assert!(document.is_dirty());
    assert_eq!(document.title(), "saved.pdf");
    assert!(Arc::ptr_eq(&original, &document.edits.strokes(0)[0]));
    document.undo();
    assert!(!document.is_dirty());
    document.undo();
    assert!(document.is_dirty());
    document.redo();
    assert!(!document.is_dirty());
}

#[test]
fn failed_and_stale_saves_keep_dirty_edits_redo_and_the_previous_target() {
    use crate::pdf::{SaveResponse, SavedPdf};
    let mut document = document();
    add(&mut document);
    add(&mut document);
    document.undo();
    let revision = document.revision();
    document.pending_save = Some((100, revision.current));
    document.saved(SaveResponse {
        document: None,
        request: 99,
        revision: revision.current,
        result: Ok(SavedPdf {
            path: "stale.pdf".into(),
            fingerprint: crate::storage::FileFingerprint::of(b"stale"),
        }),
    });
    assert!(document.is_saving());
    assert!(document.save_path().is_none());
    document.saved(SaveResponse {
        document: None,
        request: 100,
        revision: revision.current,
        result: Err("disk full".into()),
    });
    assert_eq!(document.revision(), revision);
    assert!(!document.is_saving());
    assert!(document.is_dirty() && document.can_redo() && document.can_save());
    assert!(document.save_path().is_none());
    assert!(document.notice().unwrap().contains("disk full"));
    document.close();
    document.saved(SaveResponse {
        document: None,
        request: 100,
        revision: revision.current,
        result: Ok(SavedPdf {
            path: "late.pdf".into(),
            fingerprint: crate::storage::FileFingerprint::of(b"late"),
        }),
    });
    assert!(!document.can_close());
    assert!(document.save_path().is_none());
}
