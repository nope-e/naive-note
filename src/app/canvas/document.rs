//! PDF lifecycle, worker responses and preview state for the current workspace.

mod saving;
#[cfg(test)]
mod tests;

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use winit::event_loop::EventLoopProxy;

use crate::{
    app::UserEvent,
    document::{
        DocumentRevision, EditCommand, EditHistory, PageGeometry, ReferenceLayer, ReferenceLines,
        ReferencePattern, StrokeRecord,
    },
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
    notice: Option<String>,
    loaded_request: Option<u64>,
    edits: EditHistory,
    pending_save: Option<(u64, u64)>,
    saved_target: Option<(PathBuf, crate::storage::FileFingerprint)>,
    document_id: uuid::Uuid,
    saving: saving::SavingState,
    global_reference: ReferenceLines,
    reference_metadata: Option<ReferenceLayer>,
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
            loaded_request: None,
            edits: EditHistory::default(),
            pending_save: None,
            saved_target: None,
            document_id: uuid::Uuid::new_v4(),
            saving: saving::SavingState::default(),
            global_reference: ReferenceLines::default(),
            reference_metadata: None,
        }
    }

    pub fn open(&mut self, path: PathBuf) {
        self.saving.recovering = None;
        self.start_open(path);
    }

    pub fn recover(&mut self, path: PathBuf) {
        self.saving.recovering = Some(path.clone());
        self.start_open(path);
    }

    fn start_open(&mut self, path: PathBuf) {
        self.notice = None;
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
            PdfResponse::Saved(response) => {
                self.saved(response);
                None
            }
            PdfResponse::Opened(response) => self.opened(response),
            PdfResponse::Tiles(response) => {
                if let Some(loaded) = self.loaded.as_mut()
                    && let Err(error) = loaded.preview.receive(response)
                {
                    self.set_error(format!("高清渲染失败：{error}"));
                }
                None
            }
            PdfResponse::DraftRemoved { .. } => None,
        }
    }

    fn opened(&mut self, response: OpenResponse) -> Option<Vec<PageGeometry>> {
        if self.pending != Some(response.request) {
            return None;
        }
        self.pending = None;
        match response.result {
            Ok(pdf) => {
                let recovering = self.saving.recovering.take();
                self.discard_recovery_except(recovering.as_deref());
                self.saving.reset();
                self.edits = EditHistory::default();
                self.reference_metadata = pdf.reference_lines;
                self.pending_save = None;
                self.saved_target = if recovering.is_some() {
                    self.saving.copy = Some((pdf.path.clone(), Some(pdf.fingerprint)));
                    self.saving.restored = true;
                    None
                } else {
                    Some((pdf.path.clone(), pdf.fingerprint))
                };
                self.document_id = uuid::Uuid::new_v4();
                let pages = pdf.pages.clone();
                self.message = Some(format!(
                    "{} | {} | Continuous reading",
                    pdf.path.display(),
                    pdf.edit_data_status,
                ));
                let preview = PdfPreview::new(
                    response.request,
                    Arc::clone(&pdf.first_page),
                    pdf.rendered_reference_lines,
                );
                self.loaded = Some(OpenDocument { pdf, preview });
                self.loaded_request = Some(response.request);
                Some(pages)
            }
            Err(error) => {
                self.saving.recovering = None;
                self.set_error(format!("打开失败：{error}"));
                None
            }
        }
    }

    pub fn close(&mut self) {
        self.discard_recovery();
        self.worker.close();
        self.pending = None;
        self.loaded = None;
        self.message = None;
        self.notice = None;
        self.loaded_request = None;
        self.edits = EditHistory::default();
        self.reference_metadata = None;
        self.pending_save = None;
        self.saved_target = None;
        self.document_id = uuid::Uuid::new_v4();
        self.saving.reset();
    }

    pub fn can_ink(&self) -> bool {
        self.pending.is_none()
    }

    pub fn set_global_reference(&mut self, lines: ReferenceLines) {
        self.global_reference = lines;
    }

    pub fn reference_layer(&self) -> ReferenceLayer {
        self.edits
            .reference_lines
            .or(self.reference_metadata)
            .unwrap_or_else(|| ReferenceLayer {
                lines: if self.is_loaded() {
                    ReferenceLines {
                        pattern: ReferencePattern::None,
                        ..ReferenceLines::default()
                    }
                } else {
                    self.global_reference
                },
                embed_in_pdf: !self.is_loaded(),
            })
    }

    pub fn reference_uses_global(&self) -> bool {
        !self.is_loaded()
            && self.edits.reference_lines.is_none()
            && self.reference_metadata.is_none()
    }

    pub fn set_reference_layer(&mut self, layer: ReferenceLayer) {
        if self.can_ink()
            && layer.lines.validate().is_ok()
            && layer != self.reference_layer()
            && self.edits.execute(EditCommand::ReferenceLines {
                before: Some(self.reference_layer()),
                after: layer,
            })
        {
            self.edit_changed();
        }
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
        self.revision().is_dirty() || self.saving.restored
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
            || self.saved_target.is_some()
            || self.pending.is_some()
            || self.edits.can_undo()
            || self.edits.can_redo()
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded.is_some()
    }

    pub fn label(&self, page: u32) -> String {
        let label = self.loaded.as_ref().map_or_else(
            || "Draft page 1 / 1".to_owned(),
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

    pub(super) fn set_error(&mut self, message: String) {
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
        if let Some((path, _)) = &self.saved_target {
            return path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
        }
        if self.saving.restored {
            return "恢复的未命名草稿".to_owned();
        }
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
        if self.is_saving() {
            return "正在保存 PDF…".to_owned();
        }
        if self.pending.is_some() {
            return "正在打开 PDF…".to_owned();
        }
        if self.saving.paused {
            return "外部修改冲突 · 自动保存已暂停 · 请另存为".to_owned();
        }
        let count = self.stroke_count();
        if self.is_dirty() {
            if self.saved_target.is_none()
                && self.saving.copy.is_some()
                && self.saving.copied_revision == self.revision().current
            {
                format!("草稿副本已自动保存 · 尚未命名 · 会话笔迹 {count} 笔")
            } else {
                format!("未保存更改 · 会话笔迹 {count} 笔")
            }
        } else if self.is_loaded() || self.saved_target.is_some() {
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
        let reference_lines = self.reference_layer().lines;
        for page in &mut scene.pages {
            page.strokes = self.edits.strokes(page.index).to_vec();
            page.reference_lines = reference_lines;
        }
        if let Some(loaded) = self.loaded.as_mut() {
            loaded.preview.update(
                context,
                &self.worker,
                scene,
                current_page,
                self.pending.is_some(),
                reference_lines,
            );
        }
    }
}
