//! PDF lifecycle, worker responses and preview state for the current workspace.

mod probe;
#[cfg(test)]
mod tests;

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use winit::event_loop::EventLoopProxy;

use crate::{
    app::UserEvent,
    document::{DocumentRevision, EditCommand, EditHistory, PageGeometry, StrokeRecord},
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
    notice: Option<String>,
    pending_export: Option<(u64, u64)>,
    loaded_request: Option<u64>,
    edits: EditHistory,
}

impl CanvasDocument {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        let worker = PdfWorker::new(move |response| {
            let _ = proxy.send_event(UserEvent::PdfResponse(Box::new(response)));
        })?;
        Ok(Self::with_worker(worker))
    }

    pub(super) fn with_worker(worker: PdfWorker) -> Self {
        Self {
            worker,
            loaded: None,
            pending: None,
            message: None,
            notice: None,
            pending_export: None,
            loaded_request: None,
            edits: EditHistory::default(),
        }
    }

    pub fn open(&mut self, path: PathBuf) {
        self.notice = None;
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
                self.set_error(format!("打开失败：{error:#}"));
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
                    self.set_error(format!("高清渲染失败：{error}"));
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
                    Err(error) => self.set_error(format!("测试导出失败：{error}")),
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
                self.edits = EditHistory::default();
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
                self.set_error(format!("打开失败：{error}"));
                None
            }
        }
    }

    pub fn close(&mut self) {
        self.worker.close();
        self.pending = None;
        self.loaded = None;
        self.message = None;
        self.notice = None;
        self.pending_export = None;
        self.loaded_request = None;
        self.edits = EditHistory::default();
    }

    pub fn can_ink(&self) -> bool {
        self.pending.is_none() && self.pending_export.is_none()
    }

    pub fn add_stroke(&mut self, stroke: Arc<StrokeRecord>) {
        if self.edits.execute(EditCommand::AddStroke(stroke)) {
            self.edit_changed();
        }
    }

    pub fn can_undo(&self) -> bool {
        self.can_ink() && self.edits.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.can_ink() && self.edits.can_redo()
    }

    pub fn undo(&mut self) {
        if self.can_undo() && self.edits.undo() {
            self.edit_changed();
        }
    }

    pub fn redo(&mut self) {
        if self.can_redo() && self.edits.redo() {
            self.edit_changed();
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.revision().is_dirty()
    }

    pub fn needs_discard_confirmation(&self) -> bool {
        // Starting an open already required permission to replace dirty edits.
        // While it runs, editing is blocked and a close/exit can cancel it.
        self.is_dirty() && self.pending.is_none()
    }

    fn revision(&self) -> DocumentRevision {
        self.edits.revision()
    }

    fn edit_changed(&mut self) {
        let revision = self.revision();
        self.message = Some(format!(
            "{} session stroke(s) | Revision {} | Saved revision {} | {}",
            self.stroke_count(),
            revision.current,
            revision.saved,
            if revision.is_dirty() {
                "Unsaved changes"
            } else {
                "No unsaved changes"
            },
        ));
    }

    fn stroke_count(&self) -> usize {
        self.edits.stroke_count()
    }

    pub fn can_close(&self) -> bool {
        self.loaded.is_some()
            || self.pending.is_some()
            || self.edits.can_undo()
            || self.edits.can_redo()
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded.is_some()
    }

    pub fn label(&self, page: u32) -> String {
        let label = self.loaded.as_ref().map_or_else(
            || "Test page 1 / 1".to_owned(),
            |loaded| format!("PDF page {} / {}", page + 1, loaded.pdf.page_count),
        );
        let revision = self.revision();
        format!(
            "{label} | {} session stroke(s) | r{} / saved r{}{}",
            self.stroke_count(),
            revision.current,
            revision.saved,
            if self.is_dirty() {
                " | Unsaved changes"
            } else {
                ""
            }
        )
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    fn set_error(&mut self, message: String) {
        self.notice = Some(message.clone());
        self.message = Some(message);
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn dismiss_notice(&mut self) {
        self.notice = None;
    }

    pub fn title(&self) -> String {
        self.loaded.as_ref().map_or_else(
            || "新建文档".to_owned(),
            |loaded| {
                loaded
                    .pdf
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            },
        )
    }

    pub fn subtitle(&self) -> String {
        if self.pending.is_some() {
            return "正在打开 PDF…".to_owned();
        }
        if self.pending_export.is_some() {
            return "正在导出测试 PDF…".to_owned();
        }
        let count = self.stroke_count();
        if self.is_dirty() {
            format!("未保存更改 · 会话笔迹 {count} 笔")
        } else if self.is_loaded() {
            "PDF 文档".to_owned()
        } else {
            "草稿".to_owned()
        }
    }

    pub fn update_scene(
        &mut self,
        context: &egui::Context,
        scene: &mut CanvasScene,
        current_page: u32,
    ) {
        for page in &mut scene.pages {
            page.strokes = self.edits.strokes(page.index).to_vec();
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
