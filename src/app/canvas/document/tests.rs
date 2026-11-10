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
    }
}

fn add(document: &mut CanvasDocument) {
    let stroke = crate::ink::demo_stroke(fixture_pdf().pages[0]).unwrap();
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
