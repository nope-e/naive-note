//! Pure workspace routing with a silent PDF worker; never creates a window.

use std::sync::Arc;

use super::*;
use crate::{
    document::{CanvasViewport, PageGeometry, Rect, Rotation},
    pdf::PdfWorker,
};

fn workspace() -> CanvasWorkspace {
    CanvasWorkspace {
        view: CanvasView::draft(),
        area: CanvasArea::default(),
        document: CanvasDocument::with_worker(PdfWorker::new(|_| {}).unwrap()),
        input: CanvasInput::for_editor(),
        toolbar: WorkspaceToolbar::default(),
        open_requested: false,
        save_requested: None,
        exit_requested: false,
        transition: DocumentTransition::default(),
    }
}

fn add(workspace: &mut CanvasWorkspace) {
    let page = PageGeometry::new(Rect::new(0.0, 0.0, 600.0, 800.0), Rotation::R0, 1.0).unwrap();
    workspace.document.add_stroke(Arc::new(
        crate::ink::test_support::pressure_stroke(page).unwrap(),
    ));
}

#[test]
fn save_request_finishes_active_ink_once_and_dialog_cancel_preserves_it() {
    let mut workspace = workspace();
    let viewport = CanvasViewport::new(Rect::new(0.0, 0.0, 900.0, 700.0), 1.0, (900, 700)).unwrap();
    workspace.view.update_viewport(viewport);
    let position = viewport.logical.center();
    workspace.mouse_button(true, Some(position), egui::Modifiers::NONE, false);
    assert!(workspace.input.is_drawing());
    workspace.request_save(false);
    assert!(!workspace.input.is_drawing());
    assert!(workspace.document.is_dirty() && workspace.document.can_undo());
    assert!(workspace.take_save_request().is_some());
    assert!(workspace.take_save_request().is_none());
    workspace.save_dialog_result(None);
    workspace.mouse_button(false, Some(position), egui::Modifiers::NONE, false);
    workspace.document.undo();
    assert!(!workspace.document.is_dirty());
    assert!(!workspace.document.can_undo());
}

#[test]
fn cancelling_a_transition_keeps_strokes_and_the_redo_branch() {
    let mut workspace = workspace();
    add(&mut workspace);
    add(&mut workspace);
    workspace.document.undo();
    workspace.open_pdf("other.pdf".into());
    assert!(workspace.transition.is_pending());
    assert!(workspace.document.is_dirty());
    assert!(workspace.document.can_redo());
    assert!(workspace.transition.resolve(false).is_none());
    workspace.document.redo();
    workspace.document.undo();
    assert!(workspace.document.is_dirty());
    workspace.document.undo();
    assert!(!workspace.document.is_dirty());
    assert!(workspace.document.can_redo());
}

#[test]
fn close_discard_resets_session_and_exit_waits_for_a_decision() {
    let mut workspace = workspace();
    add(&mut workspace);
    workspace.request_exit();
    assert!(!workspace.take_exit_request());
    workspace.transition.resolve(false);
    assert!(!workspace.take_exit_request());
    workspace.request_transition(DocumentAction::Close);
    let action = workspace.transition.resolve(true).unwrap();
    workspace.apply_transition(action);
    assert!(!workspace.document.is_dirty());
    assert!(!workspace.document.can_undo() && !workspace.document.can_redo());
    workspace.request_exit();
    assert!(workspace.take_exit_request());
    assert!(!workspace.take_exit_request());
}

#[test]
fn closing_an_authorized_background_open_cancels_it_without_another_prompt() {
    let mut workspace = workspace();
    add(&mut workspace);
    workspace.open_pdf("missing.pdf".into());
    let action = workspace.transition.resolve(true).unwrap();
    workspace.apply_transition(action);
    assert!(!workspace.document.can_ink());
    workspace.request_transition(DocumentAction::Close);
    assert!(!workspace.transition.is_pending());
    assert!(!workspace.document.is_dirty());
    assert!(!workspace.document.can_close());
}

#[test]
fn pending_confirmation_cancels_active_ink_and_blocks_new_mouse_input() {
    let mut workspace = workspace();
    let viewport = CanvasViewport::new(Rect::new(0.0, 0.0, 900.0, 700.0), 1.0, (900, 700)).unwrap();
    workspace.view.update_viewport(viewport);
    let point = viewport.logical.center();
    add(&mut workspace);
    assert!(workspace.mouse_button(true, Some(point), egui::Modifiers::NONE, false));
    assert!(workspace.input.is_drawing());
    workspace.request_exit();
    assert!(!workspace.input.is_drawing());
    assert!(workspace.transition.is_pending());
    workspace.mouse_button(false, Some(point), egui::Modifiers::NONE, false);
    let before = workspace.view.transform;
    assert!(!workspace.mouse_button(true, Some(point), egui::Modifiers::CTRL, false));
    workspace.mouse_move(Point::new(point.x, point.y + 30.0), egui::Modifiers::CTRL);
    assert!(!workspace.input.is_drawing());
    assert_eq!(workspace.view.transform, before);
    assert!(workspace.document.can_undo());
    workspace.document.undo();
    assert!(!workspace.document.is_dirty());
}

#[test]
fn exit_waits_for_saved_edits_including_changes_made_after_the_first_snapshot() {
    use std::{fs, sync::mpsc, time::Duration};
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".scratch")
        .join(format!("workspace-save-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("saved.pdf");
    let (send, received) = mpsc::channel();
    let mut workspace = workspace();
    workspace.document = CanvasDocument::with_worker(
        PdfWorker::new(move |response| {
            send.send(response).unwrap();
        })
        .unwrap(),
    );
    add(&mut workspace);
    workspace.request_save(false);
    workspace.take_save_request();
    workspace.save_dialog_result(Some(path.clone()));
    add(&mut workspace); // The worker already owns the preceding revision.
    workspace.request_exit();
    assert!(!workspace.take_exit_request());
    workspace.pdf_response(received.recv_timeout(Duration::from_secs(5)).unwrap());
    assert!(!workspace.take_exit_request());
    assert!(workspace.transition.is_pending() && workspace.document.is_dirty());
    workspace.transition.wait_for_save();
    workspace.request_save(false);
    workspace.pdf_response(received.recv_timeout(Duration::from_secs(5)).unwrap());
    assert!(workspace.take_exit_request());
    assert!(!workspace.document.is_dirty());
    assert!(!workspace.transition.is_pending());
    fs::remove_file(path).unwrap();
    fs::remove_dir(directory).unwrap();
}
