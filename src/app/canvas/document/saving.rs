//! Capture an immutable revision; the PDF worker saves without resetting history.

use std::path::PathBuf;

use crate::{
    document::PageGeometry,
    pdf::{SaveResponse, SaveSnapshot},
};

use super::CanvasDocument;

impl CanvasDocument {
    pub fn can_save(&self) -> bool {
        self.pending.is_none() && !self.is_saving()
    }

    pub fn is_saving(&self) -> bool {
        self.pending_save.is_some()
    }

    pub fn save_path(&self) -> Option<&std::path::Path> {
        self.saved_target.as_ref().map(|(path, _)| path.as_path())
    }

    pub fn save(&mut self, path: PathBuf, draft_page: PageGeometry) {
        if !self.can_save() {
            return;
        }
        let revision = self.revision().current;
        let snapshot = SaveSnapshot {
            document: self.loaded_request,
            revision,
            strokes: self.edits.snapshot(),
            draft_page,
            document_id: self.document_id,
            path,
            tracked_target: self.saved_target.clone(),
        };
        self.notice = None;
        match self.worker.save(snapshot) {
            Ok(request) => {
                self.pending_save = Some((request, revision));
                self.message = Some("正在保存并校验 PDF…".to_owned());
            }
            Err(error) => self.set_error(format!("保存失败：{error:#}。可重试或另存为。")),
        }
    }

    pub(super) fn saved(&mut self, response: SaveResponse) {
        if self.pending_save != Some((response.request, response.revision))
            || self.loaded_request != response.document
        {
            return;
        }
        self.pending_save = None;
        match response.result {
            Ok(saved) => {
                self.saved_target = Some((saved.path.clone(), saved.fingerprint));
                self.edits.mark_saved(response.revision);
                self.edit_changed();
                self.message = Some(format!(
                    "已保存 {} · 修订 {}{}",
                    saved.path.display(),
                    response.revision,
                    if self.is_dirty() {
                        " · 仍有新的未保存更改"
                    } else {
                        ""
                    }
                ));
            }
            Err(error) => self.set_error(format!(
                "保存失败：{error}。可按 Ctrl+S 重试或 Ctrl+Shift+S 另存为。"
            )),
        }
    }
}
