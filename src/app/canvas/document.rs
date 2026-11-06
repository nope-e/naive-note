//! PDF lifecycle, worker responses and preview state for the current workspace.

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use winit::event_loop::EventLoopProxy;

use crate::{
    app::UserEvent,
    document::{PageGeometry, StrokeRecord},
    pdf::{LoadedPdf, OpenResponse, PdfResponse, PdfWorker},
    render::CanvasScene,
};

use super::pdf_preview::PdfPreview;

struct OpenDocument {
    pdf: LoadedPdf,
    preview: PdfPreview,
    demo_stroke: Option<Arc<StrokeRecord>>,
}

pub(super) struct CanvasDocument {
    worker: PdfWorker,
    loaded: Option<OpenDocument>,
    pending: Option<u64>,
    message: Option<String>,
    pending_export: Option<(u64, u64)>,
    loaded_request: Option<u64>,
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
            pending_export: None,
            loaded_request: None,
        })
    }

    pub fn open(&mut self, path: PathBuf) {
        self.pending_export = None;
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

    /// Returns page geometry only when a successful open replaces the document.
    pub fn receive(&mut self, response: PdfResponse) -> Option<Vec<PageGeometry>> {
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
            PdfResponse::Exported(response) => {
                if self.pending_export != Some((response.document, response.request))
                    || self.loaded_request != Some(response.document)
                {
                    return None;
                }
                self.pending_export = None;
                match response.result {
                    Ok(path) => self.open(path),
                    Err(error) => self.message = Some(format!("Test export failed: {error}")),
                }
                None
            }
        }
    }

    fn opened(&mut self, response: OpenResponse) -> Option<Vec<PageGeometry>> {
        if self.pending != Some(response.request) {
            return None;
        }
        self.pending = None;
        match response.result {
            Ok(pdf) => {
                let pages = pdf.pages.clone();
                self.message = Some(format!(
                    "{} | {} | Continuous reading",
                    pdf.path.display(),
                    pdf.edit_data_status,
                ));
                let preview = PdfPreview::new(response.request, Arc::clone(&pdf.first_page));
                self.loaded = Some(OpenDocument {
                    pdf,
                    preview,
                    demo_stroke: None,
                });
                self.loaded_request = Some(response.request);
                Some(pages)
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
        self.pending_export = None;
        self.loaded_request = None;
    }

    pub fn can_demo(&self) -> bool {
        self.loaded.is_some() && self.pending.is_none() && self.pending_export.is_none()
    }

    pub fn can_export_demo(&self) -> bool {
        self.can_demo()
            && self
                .loaded
                .as_ref()
                .is_some_and(|loaded| loaded.demo_stroke.is_some())
    }

    pub fn toggle_demo(&mut self) {
        if !self.can_demo() {
            return;
        }
        let loaded = self.loaded.as_mut().expect("loaded document");
        if loaded.demo_stroke.take().is_none() {
            match crate::ink::demo_stroke(loaded.pdf.first_page.geometry) {
                Ok(stroke) => {
                    loaded.demo_stroke = Some(Arc::new(stroke));
                    self.message = Some(
                        "Simulated pressure stroke | Export test PDF to save a separate copy"
                            .to_owned(),
                    );
                }
                Err(error) => self.message = Some(format!("{error:#}")),
            }
        } else {
            self.message = Some("Demo overlay removed".to_owned());
        }
    }

    pub fn export_demo(&mut self) {
        if !self.can_export_demo() {
            return;
        }
        let stroke = Arc::clone(self.loaded.as_ref().unwrap().demo_stroke.as_ref().unwrap());
        let document = self.loaded_request.unwrap();
        match self.worker.export_probe(document, stroke) {
            Ok(request) => {
                self.pending_export = Some((document, request));
                self.message = Some(
                    "Saving and verifying a test copy in .scratch/pdf-save-probe/...".to_owned(),
                );
            }
            Err(error) => self.message = Some(format!("{error:#}")),
        }
    }

    pub fn can_close(&self) -> bool {
        self.loaded.is_some() || self.pending.is_some()
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded.is_some()
    }

    pub fn label(&self, page: u32) -> String {
        self.loaded.as_ref().map_or_else(
            || "Test page 1 / 1".to_owned(),
            |loaded| format!("PDF page {} / {}", page + 1, loaded.pdf.page_count),
        )
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn update_scene(
        &mut self,
        context: &egui::Context,
        scene: &mut CanvasScene,
        current_page: u32,
    ) {
        if let Some(loaded) = self.loaded.as_mut() {
            if let Some(first) = scene.pages.iter_mut().find(|page| page.index == 0) {
                first.demo_stroke = loaded.demo_stroke.clone();
            }
            loaded.preview.update(
                context,
                &self.worker,
                scene,
                current_page,
                self.pending.is_some(),
            );
        }
    }
}
