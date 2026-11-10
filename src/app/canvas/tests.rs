//! Pure workspace routing with a silent PDF worker; never creates a window.

use std::sync::Arc;

use super::*;
use crate::{
    document::{CanvasViewport, PageGeometry, Rect, Rotation},
    pdf::PdfWorker,
};

fn workspace() -> CanvasWorkspace {
    let mut workspace = empty_workspace();
    workspace.apply_transition(DocumentAction::Create(workspace.view.draft_page()));
    workspace
}

fn empty_workspace() -> CanvasWorkspace {
    CanvasWorkspace {
        view: CanvasView::draft(),
        area: CanvasArea::default(),
        document: CanvasDocument::with_worker(PdfWorker::new(|_| {}).unwrap()),
        input: CanvasInput::for_editor(),
        objects: object_editing::ObjectEditing::default(),
        toolbar: WorkspaceToolbar::default(),
        has_document: false,
        new_document: NewDocumentDialog::default(),
        open_requested: false,
        save_requested: None,
        exit_requested: false,
        transition: DocumentTransition::default(),
        persistence: WorkspacePersistence::default(),
    }
}

#[test]
fn workspace_object_input_commits_batches_and_cancels_previews_without_losing_history() {
    use ink_input::CanvasTool;
    let mut workspace = workspace();
    let viewport = CanvasViewport::new(Rect::new(0., 0., 900., 700.), 1.5, (1350, 1050)).unwrap();
    workspace.view.update_viewport(viewport);
    let a = viewport.logical_to_physical(viewport.logical.center());
    let b = Point::new(a.x + 90., a.y);
    for point in [a, b] {
        assert!(workspace.mouse_button(true, Some(point), egui::Modifiers::NONE, false));
        assert!(workspace.mouse_button(false, Some(point), egui::Modifiers::NONE, false));
    }
    assert_eq!(workspace.document.strokes(0).len(), 2);
    workspace.input.tool = CanvasTool::Select;
    workspace.mouse_button(true, Some(a), egui::Modifiers::NONE, false);
    workspace.mouse_button(false, Some(a), egui::Modifiers::NONE, false);
    assert_eq!(workspace.objects.selected_count(), 1);
    workspace.objects.delete_selected(&mut workspace.document);
    assert_eq!(workspace.document.strokes(0).len(), 1);
    workspace.document.undo();
    let before = workspace.document.revision();
    workspace.input.tool = CanvasTool::Eraser;
    workspace.mouse_button(true, Some(a), egui::Modifiers::NONE, false);
    workspace.mouse_move(b, egui::Modifiers::NONE);
    assert_eq!(
        workspace.document.strokes(0).len(),
        2,
        "eraser previews have not committed"
    );
    workspace.cancel_input();
    assert_eq!(workspace.document.revision(), before);
    workspace.mouse_button(true, Some(a), egui::Modifiers::NONE, false);
    workspace.mouse_move(b, egui::Modifiers::NONE);
    workspace.mouse_button(false, Some(b), egui::Modifiers::NONE, false);
    assert!(workspace.document.strokes(0).is_empty());
    workspace.document.undo();
    assert_eq!(workspace.document.strokes(0).len(), 2);
    assert_eq!(workspace.document.revision(), before);
    workspace.document.redo();
    assert!(workspace.document.strokes(0).is_empty());
}

#[test]
fn welcome_has_no_active_document_and_blocks_ink_navigation_and_save() {
    let mut workspace = empty_workspace();
    assert!(!workspace.has_document);
    assert!(workspace.new_document.open);
    let viewport = CanvasViewport::new(Rect::new(0., 0., 900., 700.), 1., (900, 700)).unwrap();
    workspace.view.update_viewport(viewport);
    let point = viewport.logical.center();
    let transform = workspace.view.transform;
    for modifiers in [
        egui::Modifiers::NONE,
        egui::Modifiers::CTRL,
        egui::Modifiers::ALT,
    ] {
        assert!(!workspace.mouse_button(true, Some(point), modifiers, false));
        workspace.mouse_move(Point::new(point.x, point.y + 20.0), modifiers);
        workspace.mouse_button(false, Some(point), modifiers, false);
    }
    assert!(!workspace.input.is_drawing());
    assert_eq!(workspace.view.transform, transform);
    assert!(!workspace.document.is_dirty());
    workspace.request_save(false);
    assert!(workspace.take_save_request().is_none());
    workspace.request_exit();
    assert!(workspace.take_exit_request());
}

#[test]
fn creating_uses_the_requested_page_and_closing_returns_to_welcome() {
    let mut workspace = empty_workspace();
    let page = PageGeometry::new(Rect::new(0., 0., 1200., 800.), Rotation::R0, 1.).unwrap();
    workspace.request_transition(DocumentAction::Create(page));
    assert!(workspace.has_document);
    assert!(!workspace.new_document.open);
    assert_eq!(workspace.view.draft_page(), page);
    assert!(!workspace.view.fit_to_page);
    assert_eq!(workspace.view.transform.zoom(), 1.0);
    workspace.request_transition(DocumentAction::Close);
    assert!(!workspace.has_document);
    assert!(workspace.new_document.open);
    assert!(workspace.canvas_blocked());
}

#[test]
fn new_document_chooser_preserves_edits_until_creation_is_confirmed() {
    let mut workspace = workspace();
    add(&mut workspace);
    add(&mut workspace);
    workspace.document.undo();
    let original_page = workspace.view.draft_page();
    let page = PageGeometry::new(Rect::new(0., 0., 900., 600.), Rotation::R0, 1.).unwrap();
    workspace.new_document.open = true;
    assert!(workspace.document.can_redo());
    assert!(workspace.canvas_blocked());
    workspace.request_transition(DocumentAction::Create(page));
    assert!(workspace.transition.is_pending());
    assert_eq!(workspace.view.draft_page(), original_page);
    workspace.transition.resolve(false);
    assert!(workspace.document.can_redo());
    // The confirmation's Save must work while the chooser is still open.
    workspace.request_transition(DocumentAction::Create(page));
    workspace.transition.wait_for_save();
    workspace.request_save(false);
    assert!(workspace.take_save_request().is_some());
    workspace.save_dialog_result(None);
    assert!(workspace.transition.is_pending());
    let action = workspace.transition.resolve(true).unwrap();
    workspace.apply_transition(action);
    assert_eq!(workspace.view.draft_page(), page);
    assert!(!workspace.document.is_dirty());
    assert!(!workspace.document.can_undo() && !workspace.document.can_redo());
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

struct AutosaveFixture {
    workspace: CanvasWorkspace,
    responses: std::sync::mpsc::Receiver<PdfResponse>,
    root: PathBuf,
}

impl AutosaveFixture {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".scratch")
            .join(format!("autosave-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("drafts")).unwrap();
        let (sender, responses) = std::sync::mpsc::channel();
        let mut workspace = workspace();
        workspace.document = CanvasDocument::with_worker(
            PdfWorker::new(move |response| {
                let _ = sender.send(response);
            })
            .unwrap(),
        );
        workspace.document.set_draft_directory(root.join("drafts"));
        Self {
            workspace,
            responses,
            root,
        }
    }

    fn receive(&mut self) {
        let response = self
            .responses
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        self.workspace.pdf_response(response);
    }

    fn tick(&mut self) -> bool {
        self.workspace
            .maintenance(self.workspace.autosave_deadline())
    }
}

impl Drop for AutosaveFixture {
    fn drop(&mut self) {
        // Wait for queued work before removing only files inside this fixture.
        self.workspace.document = CanvasDocument::with_worker(PdfWorker::new(|_| {}).unwrap());
        for directory in [self.root.join("drafts"), self.root.clone()] {
            for entry in std::fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_file() {
                    std::fs::remove_file(path).unwrap();
                }
            }
        }
        std::fs::remove_dir(self.root.join("drafts")).unwrap();
        std::fs::remove_dir(&self.root).unwrap();
    }
}

#[test]
fn autosave_keeps_active_ink_does_not_queue_and_draft_still_requires_a_name() {
    let mut fixture = AutosaveFixture::new();
    assert!(!fixture.tick()); // Empty drafts never write a recovery PDF.
    add(&mut fixture.workspace);
    let viewport = CanvasViewport::new(Rect::new(0., 0., 900., 700.), 1., (900, 700)).unwrap();
    fixture.workspace.view.update_viewport(viewport);
    fixture.workspace.mouse_button(
        true,
        Some(viewport.logical.center()),
        egui::Modifiers::NONE,
        false,
    );
    assert!(fixture.workspace.input.is_drawing());
    assert!(fixture.tick());
    assert!(fixture.workspace.input.is_drawing());
    assert!(fixture.workspace.take_save_request().is_none());
    assert!(!fixture.tick()); // One save in flight, even across another period.
    add(&mut fixture.workspace);
    fixture.receive();
    assert!(fixture.workspace.document.is_dirty());
    assert!(fixture.workspace.document.save_path().is_none());
    assert!(fixture.workspace.document.can_undo());
    assert!(fixture.workspace.input.is_drawing());
    assert!(fixture.tick()); // The newer completed revision still needs saving.
    fixture.receive();
    assert!(!fixture.tick()); // Identical revision: no I/O and no response.
    assert!(fixture.responses.try_recv().is_err());
    let copies: Vec<_> = std::fs::read_dir(fixture.root.join("drafts"))
        .unwrap()
        .collect();
    assert_eq!(copies.len(), 1);
    fixture.workspace.request_save(false);
    assert!(!fixture.workspace.input.is_drawing());
    assert!(fixture.workspace.take_save_request().is_some());
    fixture
        .workspace
        .save_dialog_result(Some(fixture.root.join("named.pdf")));
    fixture.receive(); // Formal PDF save.
    fixture.receive(); // Serialized recovery-copy cleanup.
    assert!(!fixture.workspace.document.is_dirty());
    assert_eq!(
        std::fs::read_dir(fixture.root.join("drafts"))
            .unwrap()
            .count(),
        0
    );
    assert!(fixture.workspace.document.can_undo());
    fixture.workspace.document.undo();
    assert!(fixture.tick());
    fixture.receive();
    assert!(!fixture.workspace.document.is_dirty());
}

#[test]
fn named_autosave_conflict_pauses_until_save_as_and_follows_the_new_path() {
    let mut fixture = AutosaveFixture::new();
    let first = fixture.root.join("first.pdf");
    add(&mut fixture.workspace);
    fixture.workspace.save_dialog_result(Some(first.clone()));
    fixture.receive();
    add(&mut fixture.workspace);
    assert!(fixture.tick());
    add(&mut fixture.workspace); // New revision must not be acknowledged by this save.
    fixture.receive();
    assert!(fixture.workspace.document.is_dirty());
    std::fs::write(&first, b"external edit").unwrap();
    assert!(fixture.tick());
    fixture.receive();
    assert!(
        fixture
            .workspace
            .document
            .subtitle()
            .contains("自动保存已暂停")
    );
    fixture.workspace.document.dismiss_notice();
    assert!(!fixture.tick());
    assert_eq!(std::fs::read(&first).unwrap(), b"external edit");
    assert!(fixture.workspace.document.is_dirty());
    let second = fixture.root.join("second.pdf");
    fixture.workspace.save_dialog_result(Some(second.clone()));
    fixture.receive();
    assert!(!fixture.workspace.document.is_dirty());
    add(&mut fixture.workspace);
    assert!(fixture.tick());
    fixture.receive();
    assert_eq!(
        fixture.workspace.document.save_path(),
        Some(second.as_path())
    );
    assert!(!fixture.workspace.document.is_dirty());
    assert_eq!(std::fs::read(first).unwrap(), b"external edit");
    let bytes = std::fs::read(second).unwrap();
    let pdf = mupdf::pdf::PdfDocument::from_bytes(&bytes).unwrap();
    assert_eq!(pdf.load_pdf_page(0).unwrap().annotations().count(), 4);
}

#[test]
fn autosave_retries_io_failures_and_corrupt_recovery_keeps_the_current_document() {
    let mut fixture = AutosaveFixture::new();
    let drafts = fixture.root.join("drafts");
    std::fs::remove_dir(&drafts).unwrap();
    add(&mut fixture.workspace);
    assert!(fixture.tick());
    fixture.receive();
    assert!(fixture.workspace.document.is_dirty());
    assert!(fixture.workspace.document.notice().is_some());
    std::fs::create_dir(&drafts).unwrap();
    assert!(fixture.tick()); // Ordinary I/O failures retry at the next interval.
    fixture.receive();
    assert!(!fixture.tick());
    let corrupt = drafts.join(format!("{}.pdf", uuid::Uuid::new_v4()));
    std::fs::write(&corrupt, b"not a PDF").unwrap();
    fixture.workspace.document.recover(corrupt.clone());
    fixture.receive();
    assert!(fixture.workspace.document.can_undo());
    assert!(fixture.workspace.document.is_dirty());
    assert!(
        fixture
            .workspace
            .document
            .notice()
            .unwrap()
            .contains("打开失败")
    );
    fixture.workspace.document.remove_draft(corrupt.clone());
    fixture.receive();
    assert!(!corrupt.exists());
}
