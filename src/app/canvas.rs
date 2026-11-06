//! Compose workspace widgets and apply their actions to page/view state.

mod area;
mod status_bar;
mod toolbar;

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use area::{CanvasArea, CanvasResponse};
use toolbar::ViewAction;
use winit::event_loop::EventLoopProxy;

use crate::{
    document::{CanvasViewport, PageGeometry, Point, Rect, Rotation, ViewTransform},
    pdf::{LoadedPdf, OpenResponse, PdfWorker},
    render::CanvasScene,
};

use super::UserEvent;

pub(super) struct CanvasWorkspace {
    page: PageGeometry,
    view: ViewTransform,
    viewport: Option<CanvasViewport>,
    fit_to_page: bool,
    area: CanvasArea,
    worker: PdfWorker,
    pdf: Option<LoadedPdf>,
    pending: Option<u64>,
    message: Option<String>,
    open_requested: bool,
}

impl CanvasWorkspace {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        let worker = PdfWorker::new(move |response| {
            let _ = proxy.send_event(UserEvent::PdfOpened(Box::new(response)));
        })?;
        Ok(Self {
            // Synthetic A4-sized CropBox, deliberately offset in PDF user space.
            page: PageGeometry::new(Rect::new(20.0, 30.0, 615.0, 872.0), Rotation::R0, 1.0)
                .expect("valid built-in test page"),
            view: ViewTransform::default(),
            viewport: None,
            fit_to_page: true,
            area: CanvasArea::default(),
            worker,
            pdf: None,
            pending: None,
            message: None,
            open_requested: false,
        })
    }

    pub fn open_pdf(&mut self, path: PathBuf) {
        match self.worker.open(path) {
            Ok(request) => {
                self.pending = Some(request);
                self.message = Some("Loading PDF...".to_owned());
            }
            Err(error) => {
                self.pending = None;
                self.message = Some(format!("{error:#}"));
            }
        }
    }

    pub fn pdf_opened(&mut self, response: OpenResponse) {
        if self.pending != Some(response.request) {
            return;
        }
        self.pending = None;
        match response.result {
            Ok(pdf) => {
                self.page = pdf.first_page.geometry;
                self.view = ViewTransform::default();
                self.viewport = None;
                self.fit_to_page = true;
                self.area = CanvasArea::default();
                self.message = Some(format!(
                    "{} | Read-only first-page preview",
                    pdf.path.display()
                ));
                self.pdf = Some(pdf);
            }
            Err(error) => self.message = Some(format!("Open failed: {error}")),
        }
    }

    pub fn take_open_request(&mut self) -> bool {
        std::mem::take(&mut self.open_requested)
    }

    fn close_pdf(&mut self) {
        self.worker.close();
        self.pending = None;
        self.pdf = None;
        self.message = None;
        self.page = PageGeometry::new(Rect::new(20.0, 30.0, 615.0, 872.0), Rotation::R0, 1.0)
            .expect("valid built-in test page");
        self.view = ViewTransform::default();
        self.viewport = None;
        self.fit_to_page = true;
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
        let before = (self.page, self.view, self.viewport, self.fit_to_page);
        let mut action = toolbar::show(
            ui,
            self.view.zoom(),
            self.page,
            self.viewport,
            show_renderer_info,
            fps,
            self.pdf.is_some() || self.pending.is_some(),
        );
        if process_input {
            match action {
                Some(ViewAction::Open) => {
                    self.open_requested = true;
                    action = None;
                }
                Some(ViewAction::Close) => {
                    self.close_pdf();
                    action = None;
                }
                _ => {}
            }
        }
        let document_label = self.pdf.as_ref().map_or_else(
            || "Test page 1 / 1".to_owned(),
            |pdf| format!("PDF page 1 / {}", pdf.page_count),
        );
        status_bar::show(
            ui,
            self.page,
            self.view.zoom(),
            self.fit_to_page,
            self.pointer_coordinates(ui),
            &document_label,
            self.message.as_deref(),
        );

        let scene = self
            .area
            .show(ui, framebuffer, process_input, action.is_some())
            .map(|response| {
                let viewport = response.viewport;
                self.update_viewport(viewport);
                if process_input && let Some(action) = action {
                    self.apply_action(action, viewport.logical);
                }
                self.apply_interaction(response);
                CanvasScene {
                    viewport,
                    page: self.page,
                    view: self.view,
                    bitmap: self.pdf.as_ref().map(|pdf| Arc::clone(&pdf.first_page)),
                }
            });

        // Refresh toolbar/status readouts after canvas input changes the view.
        if before != (self.page, self.view, self.viewport, self.fit_to_page) {
            ui.ctx().request_repaint();
        }
        scene
    }

    fn update_viewport(&mut self, viewport: CanvasViewport) {
        if self.viewport.map(|old| old.logical) != Some(viewport.logical) {
            if self.fit_to_page || self.viewport.is_none() {
                self.view
                    .fit_page(viewport.logical, self.page.paper_size(), 24.0);
            } else if let Some(old) = self.viewport {
                self.view.reframe(old.logical, viewport.logical);
            }
        }
        self.viewport = Some(viewport);
    }

    fn apply_action(&mut self, action: ViewAction, viewport: Rect) {
        match action {
            ViewAction::Open | ViewAction::Close => {}
            ViewAction::Fit => {
                self.fit_to_page = true;
                self.view.fit_page(viewport, self.page.paper_size(), 24.0);
            }
            ViewAction::ActualSize => {
                self.fit_to_page = false;
                self.view.set_zoom_at(1.0, viewport.center());
            }
            ViewAction::Zoom(factor) => {
                self.fit_to_page = false;
                self.view
                    .set_zoom_at(self.view.zoom() * factor, viewport.center());
            }
            ViewAction::Rotate => {
                self.page = self.page.with_rotation(self.page.rotation().next());
                self.fit_to_page = true;
                self.view.fit_page(viewport, self.page.paper_size(), 24.0);
            }
        }
    }

    fn apply_interaction(&mut self, response: CanvasResponse) {
        if let Some(delta) = response.drag_delta {
            self.view.pan(delta);
            self.fit_to_page = false;
        }
        if let Some(zoom) = response.zoom {
            self.view
                .set_zoom_at(self.view.zoom() * zoom.factor, zoom.anchor);
            self.fit_to_page = false;
        }
        if let Some(delta) = response.scroll_delta {
            self.view.pan(delta);
            self.fit_to_page = false;
        }
    }

    fn pointer_coordinates(&self, ui: &egui::Ui) -> Option<(Point, Point)> {
        let pointer = ui.input(|input| input.pointer.hover_pos())?;
        let logical = Point::new(f64::from(pointer.x), f64::from(pointer.y));
        if !self.viewport?.logical.contains(logical) {
            return None;
        }
        let paper = self.view.logical_to_paper(logical);
        self.page
            .contains_paper(paper)
            .then(|| (paper, self.page.paper_to_pdf(paper)))
    }
}
