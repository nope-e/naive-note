//! Compose workspace widgets and route document/view actions to their owners.

mod area;
mod document;
mod pdf_preview;
mod status_bar;
mod toolbar;
mod view;

use std::path::PathBuf;

use anyhow::Result;
use area::CanvasArea;
use document::CanvasDocument;
use toolbar::ToolbarAction;
use view::CanvasView;
use winit::event_loop::EventLoopProxy;

use crate::{document::Point, pdf::PdfResponse, render::CanvasScene};

use super::UserEvent;

pub(super) struct CanvasWorkspace {
    view: CanvasView,
    area: CanvasArea,
    document: CanvasDocument,
    open_requested: bool,
}

impl CanvasWorkspace {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        Ok(Self {
            view: CanvasView::default(),
            area: CanvasArea::default(),
            document: CanvasDocument::new(proxy)?,
            open_requested: false,
        })
    }

    pub fn open_pdf(&mut self, path: PathBuf) {
        self.document.open(path);
    }

    pub fn pdf_response(&mut self, response: PdfResponse) {
        if let Some(page) = self.document.receive(response) {
            self.view = CanvasView::new(page);
            self.area = CanvasArea::default();
        }
    }

    pub fn take_open_request(&mut self) -> bool {
        std::mem::take(&mut self.open_requested)
    }

    fn close_pdf(&mut self) {
        self.document.close();
        self.view = CanvasView::default();
        self.area = CanvasArea::default();
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        show_renderer_info: bool,
        fps: Option<f64>,
        framebuffer: (u32, u32),
        process_input: bool,
    ) -> Option<CanvasScene> {
        let before = self.view;
        let mut action = toolbar::show(
            ui,
            self.view.transform.zoom(),
            self.view.page,
            self.view.viewport,
            show_renderer_info,
            fps,
            toolbar::DocumentActions {
                can_close: self.document.can_close(),
                can_demo: self.document.can_demo(),
                can_export_demo: self.document.can_export_demo(),
            },
        );
        if process_input {
            match action {
                Some(ToolbarAction::Open) => {
                    self.open_requested = true;
                    action = None;
                }
                Some(ToolbarAction::Close) => {
                    self.close_pdf();
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
                _ => {}
            }
        }
        let document_label = self.document.label();
        let pointer = ui
            .input(|input| input.pointer.hover_pos())
            .and_then(|pointer| {
                self.view
                    .pointer_coordinates(Point::new(f64::from(pointer.x), f64::from(pointer.y)))
            });
        status_bar::show(
            ui,
            self.view.page,
            self.view.transform.zoom(),
            self.view.fit_to_page,
            pointer,
            &document_label,
            self.document.message(),
        );

        let scene = self
            .area
            .show(ui, framebuffer, process_input, action.is_some())
            .map(|response| {
                let viewport = response.viewport;
                self.view.update_viewport(viewport);
                if process_input && let Some(ToolbarAction::View(action)) = action {
                    self.view.apply_action(action, viewport.logical);
                }
                self.view.apply_interaction(response);
                let mut scene = CanvasScene {
                    viewport,
                    page: self.view.page,
                    view: self.view.transform,
                    bitmap: None,
                    tiles: Vec::new(),
                    demo_stroke: None,
                };
                self.document.update_scene(ui.ctx(), &mut scene);
                scene
            });

        // Refresh toolbar/status readouts after canvas input changes the view.
        if before != self.view {
            ui.ctx().request_repaint();
        }
        scene
    }
}
