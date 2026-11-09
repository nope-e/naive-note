//! PDF lifecycle, worker responses and preview state for the current workspace.

mod probe;

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

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
    strokes: BTreeMap<u32, Vec<Arc<StrokeRecord>>>,
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
            strokes: BTreeMap::new(),
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
                self.strokes.clear();
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
        self.strokes.clear();
    }

    pub fn can_ink(&self) -> bool {
        self.pending.is_none() && self.pending_export.is_none()
    }

    pub fn add_stroke(&mut self, stroke: Arc<StrokeRecord>) {
        self.strokes
            .entry(stroke.page_index)
            .or_default()
            .push(stroke);
        self.message = Some(format!(
            "{} session stroke(s) | Not saved",
            self.stroke_count()
        ));
    }

    fn stroke_count(&self) -> usize {
        self.strokes.values().map(Vec::len).sum()
    }

    pub fn can_close(&self) -> bool {
        self.loaded.is_some() || self.pending.is_some()
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded.is_some()
    }

    pub fn label(&self, page: u32) -> String {
        let label = self.loaded.as_ref().map_or_else(
            || "Test page 1 / 1".to_owned(),
            |loaded| format!("PDF page {} / {}", page + 1, loaded.pdf.page_count),
        );
        let count = self.stroke_count();
        if count > 0 {
            format!("{label} | {count} unsaved stroke(s)")
        } else {
            label
        }
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
        for page in &mut scene.pages {
            if let Some(strokes) = self.strokes.get(&page.index) {
                page.strokes = strokes.clone();
            }
        }
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
