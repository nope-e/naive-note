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
        exit_requested: false,
        transition: DocumentTransition::default(),
    }
}

fn add(workspace: &mut CanvasWorkspace) {
    let page = PageGeometry::new(Rect::new(0.0, 0.0, 600.0, 800.0), Rotation::R0, 1.0).unwrap();
    workspace
        .document
        .add_stroke(Arc::new(crate::ink::demo_stroke(page).unwrap()));
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
