//! Compose workspace widgets and route document/view actions to their owners.

mod area;
mod document;
mod ink_input;
mod new_document;
mod object_editing;
mod page_expansion;
mod pdf_preview;
mod persistence;
mod scrollbars;
mod status_bar;
#[cfg(test)]
mod tests;
mod toolbar;
mod transition;
mod view;

use std::{path::PathBuf, time::Instant};

use anyhow::Result;
use area::CanvasArea;
use document::CanvasDocument;
use ink_input::CanvasInput;
use new_document::{NewDocumentAction, NewDocumentDialog};
use persistence::{AutosaveTimer, RecoveryAction, WorkspacePersistence};
use toolbar::{ToolbarAction, WorkspaceToolbar};
use transition::{DocumentAction, DocumentTransition, TransitionDecision};
use view::CanvasView;
use winit::event_loop::EventLoopProxy;

use crate::{document::Point, pdf::PdfResponse, render::CanvasScene};

use super::UserEvent;

pub(super) struct CanvasWorkspace {
    view: CanvasView,
    area: CanvasArea,
    document: CanvasDocument,
    input: CanvasInput,
    objects: object_editing::ObjectEditing,
    toolbar: WorkspaceToolbar,
    has_document: bool,
    new_document: NewDocumentDialog,
    open_requested: bool,
    save_requested: Option<PathBuf>,
    exit_requested: bool,
    transition: DocumentTransition,
    persistence: WorkspacePersistence,
}

impl CanvasWorkspace {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        let mut document = CanvasDocument::new(proxy.clone())?;
        let persistence = match WorkspacePersistence::new(proxy) {
            Ok(persistence) => persistence,
            Err(error) => {
                document.set_error(format!("本地数据不可用：{error:#}"));
                WorkspacePersistence::default()
            }
        };
        if let Some(root) = &persistence.root {
            document.set_draft_directory(root.join("drafts"));
        }
        Ok(Self {
            view: CanvasView::draft(),
            area: CanvasArea::default(),
            document,
            input: CanvasInput::for_editor(),
            objects: object_editing::ObjectEditing::default(),
            toolbar: WorkspaceToolbar::default(),
            has_document: false,
            new_document: NewDocumentDialog::default(),
            open_requested: false,
            save_requested: None,
            exit_requested: false,
            transition: DocumentTransition::default(),
            persistence,
        })
    }

    pub fn open_pdf(&mut self, path: PathBuf) {
        self.request_transition(DocumentAction::Open(path));
    }

    pub fn request_exit(&mut self) {
        self.request_transition(DocumentAction::Exit);
    }

    pub fn take_exit_request(&mut self) -> bool {
        std::mem::take(&mut self.exit_requested)
    }

    fn request_transition(&mut self, action: DocumentAction) {
        self.input.cancel_drawing();
        if let Some(action) = self.transition.request(
            action,
            self.document.needs_discard_confirmation() || self.document.is_saving(),
        ) {
            self.apply_transition(action);
        }
        if self.document.is_saving() {
            self.transition.wait_for_save();
        }
    }

    fn apply_transition(&mut self, action: DocumentAction) {
        match action {
            DocumentAction::Open(path) => self.document.open(path),
            DocumentAction::Recover(path) => self.document.recover(path),
            DocumentAction::Create(page) => {
                self.close_pdf();
                self.view = CanvasView::blank(page);
                self.has_document = true;
                self.new_document.open = false;
            }
            DocumentAction::Close => self.close_pdf(),
            DocumentAction::Exit => {
                self.document.discard_recovery();
                self.exit_requested = true;
            }
        }
    }

    pub fn pdf_response(&mut self, response: PdfResponse) {
        if let PdfResponse::DraftRemoved { path, result } = response {
            self.persistence.deleting.retain(|pending| pending != &path);
            match result {
                Ok(()) => self.persistence.drafts.retain(|draft| draft.path != path),
                Err(error) => self.document.set_error(error),
            }
            return;
        }
        let opened_path = match &response {
            PdfResponse::Opened(response) => {
                response.result.as_ref().ok().map(|pdf| pdf.path.clone())
            }
            _ => None,
        };
        let was_saving = self.document.is_saving();
        let saved_ok = matches!(&response, PdfResponse::Saved(response) if response.result.is_ok());
        if let Some(page) = self.document.receive(response) {
            self.objects.clear();
            self.persistence
                .drafts
                .retain(|draft| Some(&draft.path) != opened_path.as_ref());
            self.input.cancel_drawing();
            self.has_document = true;
            self.new_document.open = false;
            self.view = CanvasView::new(page);
            self.area = CanvasArea::default();
        }
        if was_saving
            && !self.document.is_saving()
            && let Some(action) = self
                .transition
                .save_finished(saved_ok && !self.document.is_dirty())
        {
            self.apply_transition(action);
        }
    }

    fn request_save(&mut self, save_as: bool) {
        if !self.has_document
            || (self.new_document.open && !self.transition.is_pending())
            || !self.document.can_save()
        {
            return;
        }
        if let Some(stroke) = self.input.finish_for_save() {
            self.document.add_stroke(stroke);
        }
        self.flush_object_events();
        if !save_as && let Some(path) = self.document.save_path() {
            let path = path.to_owned();
            self.document.save(path, self.view.draft_page());
            if !self.document.is_saving() {
                self.transition.save_finished(false);
            }
        } else {
            self.save_requested = Some(
                self.document
                    .save_path()
                    .map_or_else(|| PathBuf::from("未命名.pdf"), PathBuf::from),
            );
        }
    }

    pub fn take_save_request(&mut self) -> Option<PathBuf> {
        self.save_requested.take()
    }

    pub fn save_dialog_result(&mut self, path: Option<PathBuf>) {
        if let Some(path) = path {
            self.document.save(path, self.view.draft_page());
        }
        if !self.document.is_saving() {
            self.transition.save_finished(false);
        }
    }

    pub fn take_open_request(&mut self) -> bool {
        std::mem::take(&mut self.open_requested)
    }

    fn close_pdf(&mut self) {
        self.input.cancel_drawing();
        self.flush_object_events();
        self.objects.clear();
        self.document.close();
        self.view = CanvasView::draft();
        self.area = CanvasArea::default();
        self.has_document = false;
        self.new_document.open = true;
    }

    fn canvas_blocked(&self) -> bool {
        !self.has_document
            || self.new_document.open
            || self.transition.is_pending()
            || self.persistence.recovery_open
    }

    pub fn cancel_input(&mut self) {
        self.input.cancel();
        self.flush_object_events();
    }

    fn flush_object_events(&mut self) {
        self.objects
            .receive(self.input.take_object_events(), &mut self.document);
    }

    fn sync_document_pages(&mut self) {
        self.view
            .set_pages(self.document.pages(self.view.draft_page()));
        self.objects.prune(&self.document);
    }

    fn append_page(&mut self) {
        if self.canvas_blocked()
            || self.view.current_page() + 1 != self.view.page_count()
            || !self.document.append_page(self.view.draft_page())
        {
            return;
        }
        self.objects.clear();
        self.sync_document_pages();
        self.view.go_to_page(self.view.page_count() - 1);
    }

    pub fn pen_sample(&mut self, event: crate::platform::PenEvent, over_ui: bool) -> bool {
        let blocked = self.canvas_blocked();
        let can_ink = self.document.can_ink() && !blocked;
        self.input.ui_blocked = over_ui || blocked;
        let (ui_owned, stroke) =
            self.input
                .pen(event.sample, &mut self.view, can_ink, event.modifiers);
        self.expand_for_ink(stroke.as_deref());
        if let Some(stroke) = stroke {
            self.document.add_stroke(stroke);
        }
        self.flush_object_events();
        ui_owned
    }

    pub fn mouse_button(
        &mut self,
        down: bool,
        position: Option<Point>,
        modifiers: egui::Modifiers,
        over_ui: bool,
    ) -> bool {
        let blocked = self.canvas_blocked();
        let can_ink = self.document.can_ink() && !blocked;
        self.input.ui_blocked = over_ui || blocked;
        let (owned, stroke) =
            self.input
                .mouse_button(down, position, &mut self.view, can_ink, modifiers);
        self.expand_for_ink(stroke.as_deref());
        if let Some(stroke) = stroke {
            self.document.add_stroke(stroke);
        }
        self.flush_object_events();
        owned
    }

    pub fn mouse_move(&mut self, position: Point, modifiers: egui::Modifiers) {
        let stroke = self.input.mouse_move(position, &mut self.view, modifiers);
        self.expand_for_ink(stroke.as_deref());
        if let Some(stroke) = stroke {
            self.document.add_stroke(stroke);
        }
        self.flush_object_events();
    }

    pub fn modifiers_changed(&mut self, modifiers: egui::Modifiers) {
        if let Some(stroke) = self.input.modifiers_changed(modifiers, &mut self.view) {
            self.document.add_stroke(stroke);
        }
        self.flush_object_events();
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        show_renderer_info: bool,
        fps: Option<f64>,
        framebuffer: (u32, u32),
        process_input: bool,
    ) -> Option<CanvasScene> {
        self.flush_object_events();
        self.objects.prune(&self.document);
        let before = (
            self.view.transform,
            self.view.current_page(),
            self.view.page(),
        );
        let title = if self.has_document {
            self.document.title()
        } else {
            "Naive Note".to_owned()
        };
        let subtitle = if self.has_document {
            self.document.subtitle()
        } else {
            "打开或新建文档".to_owned()
        };
        let interact =
            process_input && !self.transition.is_pending() && !self.persistence.recovery_open;
        let mut action = self.toolbar.show(
            ui,
            &toolbar::ToolbarContext {
                title: &title,
                has_document: self.has_document,
                dialog_open: self.new_document.open,
                subtitle: &subtitle,
                notice: self.document.notice(),
                zoom: self.view.transform.zoom(),
                bottom_inset: if show_renderer_info { 140.0 } else { 0.0 },
                style: self.input.current_style(),
                document: toolbar::DocumentActions {
                    can_close: self.has_document || self.document.can_close(),
                    can_save: self.has_document
                        && !self.new_document.open
                        && self.document.can_save(),
                    can_undo: !self.new_document.open && self.document.can_undo(),
                    can_redo: !self.new_document.open && self.document.can_redo(),
                    current_page: self.view.current_page(),
                    page_count: self.view.page_count(),
                    tool: self.input.tool,
                    mouse_pen: self.input.mouse_pen,
                    smoothing: self.input.smoothing,
                    recovery_count: self.persistence.drafts.len(),
                    selected_count: self.objects.selected_count(),
                },
                settings: &self.persistence.settings,
                reference_layer: self.document.reference_layer(),
                reference_uses_global: self.document.reference_uses_global(),
                can_edit_document: self.has_document
                    && !self.new_document.open
                    && self.document.can_ink(),
            },
            interact,
        );
        if interact {
            if action.is_some() {
                ui.ctx().request_repaint();
            }
            if action.is_some()
                && !matches!(
                    action,
                    Some(
                        ToolbarAction::Smoothing
                            | ToolbarAction::Save
                            | ToolbarAction::SaveAs
                            | ToolbarAction::Color(_)
                            | ToolbarAction::Width(_)
                            | ToolbarAction::PressureGamma(_)
                            | ToolbarAction::DismissNotice
                            | ToolbarAction::AutosaveInterval(_)
                            | ToolbarAction::Cursor(_)
                    )
                )
            {
                self.input.cancel_drawing();
                self.flush_object_events();
            }
            match action {
                Some(ToolbarAction::New) => {
                    self.new_document.open = true;
                    action = None;
                }
                Some(ToolbarAction::GlobalReference(lines)) => {
                    self.persistence.settings.reference_lines = lines;
                    self.document.set_global_reference(lines);
                    self.persist_settings();
                    action = None;
                }
                Some(ToolbarAction::GlobalAutoExpand(enabled)) => {
                    self.persistence.settings.auto_expand = enabled;
                    self.document.set_global_auto_expand(enabled);
                    self.persist_settings();
                    action = None;
                }
                Some(ToolbarAction::DocumentReference(layer)) => {
                    self.document.set_reference_layer(layer);
                    action = None;
                }
                Some(ToolbarAction::Save) => {
                    self.request_save(false);
                    action = None;
                }
                Some(ToolbarAction::SaveAs) => {
                    self.request_save(true);
                    action = None;
                }
                Some(ToolbarAction::Open) => {
                    self.open_requested = true;
                    action = None;
                }
                Some(ToolbarAction::Close) => {
                    self.request_transition(DocumentAction::Close);
                    action = None;
                }
                Some(ToolbarAction::Undo) => {
                    self.document.undo();
                    self.sync_document_pages();
                    action = None;
                }
                Some(ToolbarAction::Redo) => {
                    self.document.redo();
                    self.sync_document_pages();
                    action = None;
                }
                Some(ToolbarAction::AppendPage) => {
                    self.append_page();
                    action = None;
                }
                Some(ToolbarAction::Tool(tool)) => {
                    self.input.tool = tool;
                    if !tool.is_selection() {
                        self.objects.clear();
                    }
                    action = None;
                }
                Some(ToolbarAction::DeleteSelected) => {
                    self.objects.delete_selected(&mut self.document);
                    action = None;
                }
                Some(ToolbarAction::ClearSelection) => {
                    self.objects.clear();
                    action = None;
                }
                Some(ToolbarAction::SelectPage) => {
                    self.objects
                        .select_page(&self.document, self.view.current_page());
                    action = None;
                }
                Some(ToolbarAction::MousePen) => {
                    self.input.mouse_pen = !self.input.mouse_pen;
                    self.persist_settings();
                    action = None;
                }
                Some(ToolbarAction::Smoothing) => {
                    self.input.smoothing = !self.input.smoothing;
                    self.persist_settings();
                    action = None;
                }
                Some(ToolbarAction::Color(color)) => {
                    self.input.current_style_mut().color_rgb = color;
                    self.persist_settings();
                    action = None;
                }
                Some(ToolbarAction::Width(width)) => {
                    self.input.current_style_mut().max_width_pt = width;
                    self.persist_settings();
                    action = None;
                }
                Some(ToolbarAction::PressureGamma(gamma)) => {
                    self.input.current_style_mut().pressure_gamma = gamma;
                    self.persist_settings();
                    action = None;
                }
                Some(ToolbarAction::DismissNotice) => {
                    self.document.dismiss_notice();
                    action = None;
                }
                Some(ToolbarAction::AutosaveInterval(seconds)) => {
                    self.persistence.settings.autosave_seconds = seconds;
                    self.persistence.timer = AutosaveTimer::new(Instant::now(), seconds);
                    self.persist_settings();
                    action = None;
                }
                Some(ToolbarAction::Cursor(cursor)) => {
                    self.persistence.settings.cursor = cursor;
                    self.persist_settings();
                    action = None;
                }
                Some(ToolbarAction::RecoverDrafts) => {
                    self.input.cancel_drawing();
                    self.persistence.recovery_open = true;
                    action = None;
                }
                _ => {}
            }
        }
        let document_label = self.document.label(self.view.current_page());
        let pointer = ui
            .input(|input| input.pointer.hover_pos())
            .and_then(|pointer| {
                self.view
                    .pointer_coordinates(Point::new(f64::from(pointer.x), f64::from(pointer.y)))
            });
        if show_renderer_info && self.has_document {
            toolbar::renderer_info(ui, self.view.page(), self.view.viewport, fps);
            status_bar::show(
                ui,
                self.view.page(),
                self.view.transform.zoom(),
                self.view.fit_to_page,
                pointer,
                &document_label,
                self.document.message(),
            );
        }

        let view_action = match action {
            Some(ToolbarAction::View(action)) => Some(action),
            _ => None,
        };
        let dialog_bounds = ui.available_rect_before_wrap();
        let scene = if self.has_document {
            self.area
                .show(
                    ui,
                    framebuffer,
                    process_input && !self.input.is_drawing() && !self.canvas_blocked(),
                    &mut self.view,
                    view_action,
                )
                .map(|viewport| {
                    let mut scene = CanvasScene {
                        viewport,
                        pages: self.view.scenes(viewport, !self.document.is_loaded()),
                    };
                    self.document
                        .update_scene(ui.ctx(), &mut scene, self.view.current_page());
                    self.objects.prune(&self.document);
                    self.objects.decorate(&mut scene);
                    if let Some(stroke) = self.input.active()
                        && let Some(page) = scene
                            .pages
                            .iter_mut()
                            .find(|page| page.index == stroke.record.page_index)
                    {
                        page.active_stroke = Some(stroke);
                    }
                    scene
                })
        } else {
            None
        };

        if !self.transition.is_pending()
            && !self.persistence.recovery_open
            && let Some(action) = self.new_document.show(
                ui.ctx(),
                new_document::DialogContext {
                    bounds: dialog_bounds,
                    can_cancel: self.has_document,
                    loading: !self.document.can_ink(),
                    notice: self.document.notice(),
                    recovery_count: self.persistence.drafts.len(),
                },
            )
            && process_input
        {
            self.input.cancel_drawing();
            match action {
                NewDocumentAction::Open => self.open_requested = true,
                NewDocumentAction::Create(page) => {
                    self.request_transition(DocumentAction::Create(page))
                }
                NewDocumentAction::Recover => self.persistence.recovery_open = true,
                NewDocumentAction::Cancel => self.new_document.open = false,
            }
            ui.ctx().request_repaint();
        }

        // Render the modal last so it blocks all other UI. Apply a decision only
        // on the input pass: egui can repeat layout within the same frame.
        if !self.transition.is_pending()
            && let Some(recovery) = self.persistence.show_recovery(ui.ctx())
            && process_input
        {
            self.input.cancel_drawing();
            match recovery {
                RecoveryAction::Restore(path) => {
                    self.persistence.recovery_open = false;
                    self.request_transition(DocumentAction::Recover(path));
                }
                RecoveryAction::Delete(path) => {
                    self.persistence.deleting.push(path.clone());
                    self.document.remove_draft(path);
                }
            }
            ui.ctx().request_repaint();
        }
        if let Some(decision) = self.transition.show(
            ui.ctx(),
            &title,
            self.document.is_saving(),
            self.document.can_save(),
            self.document.notice(),
        ) && process_input
        {
            match decision {
                TransitionDecision::Cancel => {
                    self.transition.resolve(false);
                }
                TransitionDecision::Discard => {
                    self.input.cancel_drawing();
                    if let Some(action) = self.transition.resolve(true) {
                        self.apply_transition(action);
                    }
                }
                TransitionDecision::Save | TransitionDecision::SaveAs => {
                    self.transition.wait_for_save();
                    self.request_save(matches!(decision, TransitionDecision::SaveAs));
                }
            }
            ui.ctx().request_repaint();
        }

        // Refresh toolbar/status readouts after canvas input changes the view.
        if before
            != (
                self.view.transform,
                self.view.current_page(),
                self.view.page(),
            )
        {
            ui.ctx().request_repaint();
        }
        let cursor = (!self.canvas_blocked()).then(|| {
            let cursor = self.input.cursor(
                self.persistence.settings.cursor,
                ui.input(|input| input.modifiers),
                self.area.dragging(),
            );
            if matches!(
                cursor,
                crate::cursor::CanvasCursor::Icon(egui::CursorIcon::Crosshair)
            ) && let Some(physical) = ui.input(|input| input.pointer.hover_pos()).map(|p| {
                let dpi = f64::from(ui.ctx().pixels_per_point());
                Point::new(f64::from(p.x) * dpi, f64::from(p.y) * dpi)
            }) && let Some(icon) = self.objects.cursor(&self.document, &self.view, physical)
            {
                crate::cursor::CanvasCursor::Icon(icon)
            } else {
                cursor
            }
        });
        self.area.update_cursor(ui.ctx(), cursor);
        scene
    }
}
