//! PDF lifecycle, worker responses and preview state for the current workspace.

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use winit::event_loop::EventLoopProxy;

use crate::{
    app::UserEvent,
    document::PageGeometry,
    pdf::{LoadedPdf, OpenResponse, PdfResponse, PdfWorker},
    render::CanvasScene,
};

use super::pdf_preview::PdfPreview;

struct OpenDocument {
    pdf: LoadedPdf,
    preview: PdfPreview,
}

pub(super) struct CanvasDocument {
    worker: PdfWorker,
    loaded: Option<OpenDocument>,
    pending: Option<u64>,
    message: Option<String>,
}

impl CanvasDocument {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        let worker = PdfWorker::new(move |response| {
            let _ = proxy.send_event(UserEvent::PdfResponse(Box::new(response)));
        })?;
        Ok(Self {
            worker,
            loaded: None,
            pending: None,
            message: None,
        })
    }

    pub fn open(&mut self, path: PathBuf) {
        if let Some(loaded) = self.loaded.as_mut() {
            loaded.preview.cancel(&self.worker);
        }
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

    /// Returns geometry only when a successful open replaces the current page.
    pub fn receive(&mut self, response: PdfResponse) -> Option<PageGeometry> {
        match response {
            PdfResponse::Opened(response) => self.opened(response),
            PdfResponse::Tiles(response) => {
                if let Some(loaded) = self.loaded.as_mut()
                    && let Err(error) = loaded.preview.receive(response)
                {
                    self.message = Some(format!("Detail render failed: {error}"));
                }
                None
            }
        }
    }

    fn opened(&mut self, response: OpenResponse) -> Option<PageGeometry> {
        if self.pending != Some(response.request) {
            return None;
        }
        self.pending = None;
        match response.result {
            Ok(pdf) => {
                let page = pdf.first_page.geometry;
                self.message = Some(format!(
                    "{} | Read-only first-page preview",
                    pdf.path.display()
                ));
                self.loaded = Some(OpenDocument {
                    pdf,
                    preview: PdfPreview::new(response.request),
                });
                Some(page)
            }
            Err(error) => {
                self.message = Some(format!("Open failed: {error}"));
                None
            }
        }
    }

    pub fn close(&mut self) {
        self.worker.close();
        self.pending = None;
        self.loaded = None;
        self.message = None;
    }

    pub fn can_close(&self) -> bool {
        self.loaded.is_some() || self.pending.is_some()
    }

    pub fn label(&self) -> String {
        self.loaded.as_ref().map_or_else(
            || "Test page 1 / 1".to_owned(),
            |loaded| format!("PDF page 1 / {}", loaded.pdf.page_count),
        )
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn update_scene(&mut self, context: &egui::Context, scene: &mut CanvasScene) {
        if let Some(loaded) = self.loaded.as_mut() {
            scene.bitmap = Some(Arc::clone(&loaded.pdf.first_page));
            scene.tiles =
                loaded
                    .preview
                    .update(context, &self.worker, scene, self.pending.is_some());
        }
    }
}
