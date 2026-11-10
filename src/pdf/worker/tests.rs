use super::*;
use std::time::Duration;

#[test]
fn worker_saves_draft_revisions_detects_conflicts_and_can_save_as() {
    use crate::document::{PageGeometry, Rect, Rotation};
    use std::fs;
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".scratch")
        .join(format!("worker-save-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("draft.pdf");
    let page = PageGeometry::new(Rect::new(20., 30., 615., 872.), Rotation::R0, 1.).unwrap();
    let stroke = Arc::new(crate::ink::test_support::pressure_stroke(page).unwrap());
    let document_id = uuid::Uuid::new_v4();
    let snapshot = |path, tracked_target, revision, strokes| SaveSnapshot {
        document: None,
        revision,
        strokes,
        draft_page: page,
        reference_lines: crate::document::ReferenceLayer {
            lines: crate::document::ReferenceLines::default(),
            embed_in_pdf: true,
        },
        document_id,
        path,
        tracked_target,
        require_new_target: false,
    };
    let (send, received) = mpsc::channel();
    let worker = PdfWorker::new(move |response| {
        send.send(response).unwrap();
    })
    .unwrap();
    let request = worker
        .save(snapshot(path.clone(), None, 1, vec![stroke.clone()]))
        .unwrap();
    let PdfResponse::Saved(first) = received.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("expected save");
    };
    assert_eq!(first.request, request);
    assert_eq!(first.revision, 1);
    let first = first.result.unwrap();
    // Undo all session ink, save again using the same immutable draft base.
    worker
        .save(snapshot(
            path.clone(),
            Some((path.clone(), first.fingerprint)),
            0,
            vec![],
        ))
        .unwrap();
    let PdfResponse::Saved(second) = received.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("expected save");
    };
    let second = second.result.unwrap();
    let pdf = mupdf::pdf::PdfDocument::from_bytes(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(pdf.load_pdf_page(0).unwrap().annotations().count(), 0);
    assert_eq!(
        super::super::save::read_edit_data(&pdf)
            .unwrap()
            .unwrap()
            .document_id,
        document_id
    );
    fs::write(&path, b"external edit").unwrap();
    worker
        .save(snapshot(
            path.clone(),
            Some((path.clone(), second.fingerprint)),
            2,
            vec![stroke.clone()],
        ))
        .unwrap();
    let PdfResponse::Saved(failed) = received.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("expected save");
    };
    assert!(matches!(failed.result, Err(error) if error.conflict));
    assert_eq!(fs::read(&path).unwrap(), b"external edit");
    let mut collision = snapshot(path.clone(), None, 3, vec![stroke.clone()]);
    collision.require_new_target = true;
    worker.save(collision).unwrap();
    let PdfResponse::Saved(collision) = received.recv_timeout(Duration::from_secs(5)).unwrap()
    else {
        panic!("expected save conflict");
    };
    assert!(matches!(collision.result, Err(error) if error.conflict));
    assert_eq!(fs::read(&path).unwrap(), b"external edit");
    let copy = directory.join("copy.pdf");
    worker
        .save(snapshot(
            copy.clone(),
            Some((path.clone(), second.fingerprint)),
            2,
            vec![stroke],
        ))
        .unwrap();
    let PdfResponse::Saved(copied) = received.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("expected save");
    };
    assert_eq!(copied.result.unwrap().path, copy);
    let pdf = mupdf::pdf::PdfDocument::from_bytes(&fs::read(copy).unwrap()).unwrap();
    assert_eq!(pdf.load_pdf_page(0).unwrap().annotations().count(), 1);
    drop(worker);
    for entry in fs::read_dir(&directory).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::remove_dir(&directory).unwrap();
}

#[test]
fn queued_opens_are_superseded_and_close_invalidates_pending_work() {
    let (responses, received) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let worker = PdfWorker::new(move |response| {
        if let PdfResponse::Opened(response) = response {
            let _ = responses.send(response);
        }
        // Hold the worker at a known boundary while the test queues actions.
        let _ = wait.recv_timeout(Duration::from_secs(5));
    })
    .unwrap();
    let missing =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".scratch/no-such-pdf-for-worker-test.pdf");
    let first = worker
        .open(missing.clone(), crate::document::ReferenceLines::default())
        .unwrap();
    let response = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(response.request, first);
    assert!(response.result.is_err());
    let superseded = worker
        .open(missing.clone(), crate::document::ReferenceLines::default())
        .unwrap();
    let newest = worker
        .open(missing.clone(), crate::document::ReferenceLines::default())
        .unwrap();
    assert_ne!(superseded, newest);
    release.send(()).unwrap();
    let response = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(response.request, newest);

    // Both commands are queued while the callback above still owns the worker.
    worker
        .open(missing.clone(), crate::document::ReferenceLines::default())
        .unwrap();
    worker.close();
    release.send(()).unwrap();
    assert!(matches!(
        received.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    let reopened = worker
        .open(missing, crate::document::ReferenceLines::default())
        .unwrap();
    let response = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(response.request, reopened);
    release.send(()).unwrap();
    drop(worker);
}
