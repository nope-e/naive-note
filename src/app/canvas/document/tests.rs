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
        reference_lines: None,
        rendered_reference_lines: ReferenceLines {
            pattern: crate::document::ReferencePattern::None,
            ..ReferenceLines::default()
        },
        edit_data_status: "Existing annotations in immutable base".to_owned(),
        strokes: Vec::new(),
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
fn opened_verified_ink_is_editable_clean_and_its_first_removal_is_undoable() {
    let mut document = document();
    let mut pdf = fixture_pdf();
    let stroke = Arc::new(crate::ink::test_support::pressure_stroke(pdf.pages[0]).unwrap());
    pdf.strokes.push(stroke.clone());
    document.pending = Some(1);
    assert!(
        document
            .opened(OpenResponse {
                request: 1,
                result: Ok(pdf)
            })
            .is_some()
    );
    assert!(!document.is_dirty() && !document.can_undo());
    assert_eq!(document.strokes(0).len(), 1);
    document.remove_strokes(&std::collections::HashSet::from([stroke.id]));
    assert!(document.is_dirty() && document.strokes(0).is_empty());
    document.undo();
    assert!(!document.is_dirty());
    assert!(Arc::ptr_eq(&document.strokes(0)[0], &stroke));
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

#[test]
fn recovered_copy_remains_unnamed_and_requires_confirmation_even_without_new_edits() {
    let mut document = document();
    document.pending = Some(12);
    document.saving.recovering = Some("recovered.pdf".into());
    let mut pdf = fixture_pdf();
    pdf.path = "recovered.pdf".into();
    assert!(
        document
            .opened(OpenResponse {
                request: 12,
                result: Ok(pdf)
            })
            .is_some()
    );
    assert!(document.is_loaded());
    assert!(document.save_path().is_none());
    assert!(document.is_dirty() && document.needs_discard_confirmation());
    assert!(!document.can_undo());
    assert!(!document.autosave(fixture_pdf().pages[0]));
    assert!(document.subtitle().contains("尚未命名"));
    // Drop alone represents abnormal termination and keeps the copy for recovery.
}

#[test]
fn failed_save_as_conflict_does_not_pause_autosaving_the_unchanged_current_target() {
    let mut document = document();
    add(&mut document);
    document.saved_target = Some((
        "original.pdf".into(),
        crate::storage::FileFingerprint::of(b"original"),
    ));
    document.saving.destination = Some("other.pdf".into());
    document.pending_save = Some((100, document.revision().current));
    document.saved(crate::pdf::SaveResponse {
        document: None,
        request: 100,
        revision: document.revision().current,
        result: Err(crate::pdf::SaveError {
            message: "external edit".into(),
            conflict: true,
        }),
    });
    assert!(!document.saving.paused);
    assert_eq!(
        document.save_path(),
        Some(std::path::Path::new("original.pdf"))
    );
    assert!(document.is_dirty());
}

#[test]
fn discard_queues_cleanup_after_an_inflight_draft_save_and_ignores_its_late_response() {
    use std::{fs, sync::mpsc, time::Duration};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".scratch")
        .join(format!("draft-cleanup-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let (sender, responses) = mpsc::channel();
    let worker = PdfWorker::new(move |response| {
        let _ = sender.send(response);
    })
    .unwrap();
    let mut document = CanvasDocument::with_worker(worker);
    document.set_draft_directory(root.clone());
    add(&mut document);
    assert!(document.autosave(fixture_pdf().pages[0]));
    let old_id = document.document_id;
    document.close(); // FIFO cleanup must follow any already submitted write.
    add(&mut document);
    let revision = document.revision();
    assert_ne!(document.document_id, old_id);
    let response = responses.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(response, PdfResponse::Saved(_)));
    document.receive(response);
    assert_eq!(document.revision(), revision);
    assert!(document.save_path().is_none());
    assert!(document.saving.copy.is_none());
    let response = responses.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        response,
        PdfResponse::DraftRemoved { result: Ok(()), .. }
    ));
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    drop(document);
    fs::remove_dir(root).unwrap();
}

#[test]
fn global_reference_defaults_only_apply_to_new_documents_and_metadata_wins() {
    use crate::document::ReferencePattern;
    let mut doc = document();
    let lines = ReferenceLines {
        pattern: ReferencePattern::Dots,
        width: 32.0,
        ..ReferenceLines::default()
    };
    doc.set_global_reference(lines);
    assert_eq!(
        doc.reference_layer(),
        ReferenceLayer {
            lines,
            embed_in_pdf: true
        }
    );
    assert!(!doc.is_dirty());
    doc.pending = Some(50);
    doc.opened(OpenResponse {
        request: 50,
        result: Ok(fixture_pdf()),
    });
    let disabled = ReferenceLayer {
        lines: ReferenceLines {
            pattern: ReferencePattern::None,
            ..ReferenceLines::default()
        },
        embed_in_pdf: false,
    };
    assert_eq!(doc.reference_layer(), disabled);
    assert!(!doc.reference_uses_global());
    doc.set_global_reference(ReferenceLines {
        width: 64.0,
        ..lines
    });
    assert_eq!(doc.reference_layer(), disabled);
    assert!(!doc.is_dirty());
    let enabled = ReferenceLayer { lines, ..disabled };
    doc.set_reference_layer(enabled);
    assert_eq!(doc.reference_layer(), enabled);
    doc.undo();
    assert_eq!(doc.reference_layer(), disabled);
    doc.redo();
    assert_eq!(doc.reference_layer(), enabled);
    let metadata = ReferenceLayer {
        lines: ReferenceLines::default(),
        embed_in_pdf: true,
    };
    let mut pdf = fixture_pdf();
    pdf.reference_lines = Some(metadata);
    doc.pending = Some(51);
    doc.opened(OpenResponse {
        request: 51,
        result: Ok(pdf),
    });
    doc.set_global_reference(lines);
    assert_eq!(doc.reference_layer(), metadata);
    assert!(!doc.reference_uses_global());
    doc.close();
    assert_eq!(
        doc.reference_layer(),
        ReferenceLayer {
            lines,
            embed_in_pdf: true
        }
    );
    assert!(doc.reference_uses_global());
}

#[test]
fn saved_reference_edits_remain_undoable_and_new_edits_stay_dirty() {
    let mut doc = document();
    let original = doc.reference_layer();
    let mut changed = original;
    changed.lines.width = 32.0;
    doc.set_reference_layer(changed);
    let saved_revision = doc.revision().current;
    doc.pending_save = Some((100, saved_revision));
    doc.saving.reference_lines = Some(changed);
    let mut newer = changed;
    newer.embed_in_pdf = false;
    doc.set_reference_layer(newer);
    doc.saved(crate::pdf::SaveResponse {
        document: None,
        request: 100,
        revision: saved_revision,
        result: Ok(crate::pdf::SavedPdf {
            path: "saved.pdf".into(),
            fingerprint: crate::storage::FileFingerprint::of(b"saved"),
        }),
    });
    assert_eq!(doc.reference_layer(), newer);
    assert!(doc.is_dirty());
    doc.undo();
    assert_eq!(doc.reference_layer(), changed);
    assert!(!doc.is_dirty());
    doc.undo();
    assert_eq!(doc.reference_layer(), original);
    assert!(doc.is_dirty());
    doc.redo();
    assert_eq!(doc.reference_layer(), changed);
    assert!(!doc.is_dirty());
}

#[test]
fn first_successful_save_freezes_the_snapshot_instead_of_later_global_defaults() {
    let mut doc = document();
    let snapshot = doc.reference_layer();
    doc.pending_save = Some((100, 0));
    doc.saving.reference_lines = Some(snapshot);
    doc.set_global_reference(ReferenceLines {
        width: 64.0,
        ..ReferenceLines::default()
    });
    doc.saved(crate::pdf::SaveResponse {
        document: None,
        request: 100,
        revision: 0,
        result: Ok(crate::pdf::SavedPdf {
            path: "saved.pdf".into(),
            fingerprint: crate::storage::FileFingerprint::of(b"saved"),
        }),
    });
    assert_eq!(doc.reference_layer(), snapshot);
    assert!(!doc.reference_uses_global());
}
