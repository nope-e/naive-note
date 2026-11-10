//! Compose workspace widgets and route document/view actions to their owners.

mod area;
mod document;
mod ink_input;
mod pdf_preview;
mod scrollbars;
mod status_bar;
#[cfg(test)]
mod tests;
mod toolbar;
mod transition;
mod view;

use std::path::PathBuf;

use anyhow::Result;
use area::CanvasArea;
use document::CanvasDocument;
use ink_input::CanvasInput;
use toolbar::{ToolbarAction, WorkspaceToolbar};
use transition::{DocumentAction, DocumentTransition};
use view::CanvasView;
use winit::event_loop::EventLoopProxy;

use crate::{document::Point, pdf::PdfResponse, render::CanvasScene};

use super::UserEvent;

pub(super) struct CanvasWorkspace {
    view: CanvasView,
    area: CanvasArea,
    document: CanvasDocument,
    input: CanvasInput,
    toolbar: WorkspaceToolbar,
    open_requested: bool,
    exit_requested: bool,
    transition: DocumentTransition,
}

impl CanvasWorkspace {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        Ok(Self {
            view: CanvasView::draft(),
            area: CanvasArea::default(),
            document: CanvasDocument::new(proxy)?,
            input: CanvasInput::for_editor(),
            toolbar: WorkspaceToolbar::default(),
            open_requested: false,
            exit_requested: false,
            transition: DocumentTransition::default(),
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
        if let Some(action) = self
            .transition
            .request(action, self.document.needs_discard_confirmation())
        {
            self.apply_transition(action);
        }
    }

    fn apply_transition(&mut self, action: DocumentAction) {
        match action {
            DocumentAction::Open(path) => self.document.open(path),
            DocumentAction::Close => self.close_pdf(),
            DocumentAction::Exit => self.exit_requested = true,
        }
    }

    pub fn pdf_response(&mut self, response: PdfResponse) {
        if let Some(page) = self.document.receive(response) {
            self.input.cancel_drawing();
            self.view = CanvasView::new(page);
            self.area = CanvasArea::default();
        }
    }

    pub fn take_open_request(&mut self) -> bool {
        std::mem::take(&mut self.open_requested)
    }

    fn close_pdf(&mut self) {
        self.input.cancel_drawing();
        self.document.close();
        self.view = CanvasView::draft();
        self.area = CanvasArea::default();
    }

    pub fn cancel_input(&mut self) {
        self.input.cancel();
    }

    pub fn pen_sample(&mut self, event: crate::platform::PenEvent, over_ui: bool) -> bool {
        self.input.ui_blocked = over_ui || self.transition.is_pending();
        let (ui_owned, stroke) = self.input.pen(
            event.sample,
            &mut self.view,
            self.document.can_ink() && !self.transition.is_pending(),
            event.modifiers,
        );
        if let Some(stroke) = stroke {
            self.document.add_stroke(stroke);
        }
        ui_owned
    }

    pub fn mouse_button(
        &mut self,
        down: bool,
        position: Option<Point>,
        modifiers: egui::Modifiers,
        over_ui: bool,
    ) -> bool {
        self.input.ui_blocked = over_ui || self.transition.is_pending();
        let (owned, stroke) = self.input.mouse_button(
            down,
            position,
            &mut self.view,
            self.document.can_ink() && !self.transition.is_pending(),
            modifiers,
        );
        if let Some(stroke) = stroke {
            self.document.add_stroke(stroke);
        }
        owned
    }

    pub fn mouse_move(&mut self, position: Point, modifiers: egui::Modifiers) {
        if let Some(stroke) = self.input.mouse_move(position, &mut self.view, modifiers) {
            self.document.add_stroke(stroke);
        }
    }

    pub fn modifiers_changed(&mut self, modifiers: egui::Modifiers) {
        if let Some(stroke) = self.input.modifiers_changed(modifiers, &mut self.view) {
            self.document.add_stroke(stroke);
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        show_renderer_info: bool,
        fps: Option<f64>,
        framebuffer: (u32, u32),
        process_input: bool,
    ) -> Option<CanvasScene> {
        let before = (
            self.view.transform,
            self.view.current_page(),
            self.view.page(),
        );
        let title = self.document.title();
        let subtitle = self.document.subtitle();
        let interact = process_input && !self.transition.is_pending();
        let mut action = self.toolbar.show(
            ui,
            &toolbar::ToolbarContext {
                title: &title,
                subtitle: &subtitle,
                notice: self.document.notice(),
                zoom: self.view.transform.zoom(),
                bottom_inset: if show_renderer_info { 140.0 } else { 0.0 },
                style: &self.input.style,
                document: toolbar::DocumentActions {
                    can_close: self.document.can_close(),
                    can_undo: self.document.can_undo(),
                    can_redo: self.document.can_redo(),
                    can_demo: self.document.can_demo(),
                    can_export_demo: self.document.can_export_demo(),
                    current_page: self.view.current_page(),
                    page_count: self.view.page_count(),
                    tool: self.input.tool,
                    mouse_pen: self.input.mouse_pen,
                    smoothing: self.input.smoothing,
                },
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
                            | ToolbarAction::Color(_)
                            | ToolbarAction::Width(_)
                            | ToolbarAction::PressureGamma(_)
                            | ToolbarAction::DismissNotice
                    )
                )
            {
                self.input.cancel_drawing();
            }
            match action {
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
                    action = None;
                }
                Some(ToolbarAction::Redo) => {
                    self.document.redo();
                    action = None;
                }
                Some(ToolbarAction::DemoStroke) => {
                    self.document.toggle_demo();
                    action = None;
                }
                Some(ToolbarAction::ExportDemo) => {
                    self.document.export_demo();
                    action = None;
                }
                Some(ToolbarAction::Tool(tool)) => {
                    self.input.tool = tool;
                    action = None;
                }
                Some(ToolbarAction::MousePen) => {
                    self.input.mouse_pen = !self.input.mouse_pen;
                    action = None;
                }
                Some(ToolbarAction::Smoothing) => {
                    self.input.smoothing = !self.input.smoothing;
                    action = None;
                }
                Some(ToolbarAction::Color(color)) => {
                    self.input.style.color_rgb = color;
                    action = None;
                }
                Some(ToolbarAction::Width(width)) => {
                    self.input.style.max_width_pt = width;
                    action = None;
                }
                Some(ToolbarAction::PressureGamma(gamma)) => {
                    self.input.style.pressure_gamma = gamma;
                    action = None;
                }
                Some(ToolbarAction::DismissNotice) => {
                    self.document.dismiss_notice();
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
        if show_renderer_info {
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
        let scene = self
            .area
            .show(
                ui,
                framebuffer,
                process_input && !self.input.is_drawing() && !self.transition.is_pending(),
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
                if let Some(stroke) = self.input.active()
                    && let Some(page) = scene
                        .pages
                        .iter_mut()
                        .find(|page| page.index == stroke.record.page_index)
                {
                    page.active_stroke = Some(stroke);
                }
                scene
            });

        // Render the modal last so it blocks all other UI. Apply a decision only
        // on the input pass: egui can repeat layout within the same frame.
        if let Some(discard) = self.transition.show(ui.ctx(), &title)
            && process_input
        {
            self.input.cancel_drawing();
            if let Some(action) = self.transition.resolve(discard) {
                self.apply_transition(action);
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
        scene
    }
}
