//! Capture completed edits; manual and automatic saves share one in-flight slot.

use std::path::{Path, PathBuf};

use crate::{
    document::PageGeometry,
    pdf::{SaveResponse, SaveSnapshot},
    storage::FileFingerprint,
};

use super::CanvasDocument;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum SaveKind {
    #[default]
    Manual,
    AutoNamed,
    AutoDraft,
}

#[derive(Default)]
pub(super) struct SavingState {
    kind: SaveKind,
    pub destination: Option<PathBuf>,
    pub directory: Option<PathBuf>,
    pub copy: Option<(PathBuf, Option<FileFingerprint>)>,
    pub copied_revision: u64,
    pub restored: bool,
    pub paused: bool,
    pub recovering: Option<PathBuf>,
    pub(super) reference_lines: Option<crate::document::ReferenceLayer>,
}

impl SavingState {
    pub fn reset(&mut self) {
        *self = Self {
            directory: self.directory.take(),
            ..Self::default()
        };
    }
}

impl CanvasDocument {
    pub fn can_save(&self) -> bool {
        self.pending.is_none() && !self.is_saving()
    }
    pub fn is_saving(&self) -> bool {
        self.pending_save.is_some()
    }

    pub fn save_path(&self) -> Option<&Path> {
        self.saved_target.as_ref().map(|(path, _)| path.as_path())
    }

    pub fn set_draft_directory(&mut self, directory: PathBuf) {
        self.saving.directory = Some(directory);
    }

    pub fn save(&mut self, path: PathBuf, draft_page: PageGeometry) {
        // Recovery paths are application-owned; a formal file must live elsewhere.
        if self.saving.directory.as_ref().is_some_and(|directory| {
            path.parent()
                .is_some_and(|parent| crate::storage::same_file_path(parent, directory))
        }) {
            self.set_error("请选择草稿恢复目录之外的正式 PDF 路径".to_owned());
            return;
        }
        self.start_save(path, draft_page, SaveKind::Manual);
    }

    /// Only committed history is copied; the caller never finishes input here.
    pub fn autosave(&mut self, draft_page: PageGeometry) -> bool {
        if !self.can_save() || self.saving.paused {
            return false;
        }
        let (path, kind) = if let Some(path) = self.save_path() {
            if !self.revision().is_dirty() {
                return false;
            }
            (path.to_owned(), SaveKind::AutoNamed)
        } else {
            if self.revision().current == self.saving.copied_revision {
                return false;
            }
            let path = match &self.saving.copy {
                Some((path, _)) => path.clone(),
                None => {
                    let Some(directory) = &self.saving.directory else {
                        return false;
                    };
                    let path = directory.join(format!("{}.pdf", self.document_id));
                    self.saving.copy = Some((path.clone(), None));
                    path
                }
            };
            (path, SaveKind::AutoDraft)
        };
        self.start_save(path, draft_page, kind);
        self.is_saving()
    }

    fn start_save(&mut self, path: PathBuf, draft_page: PageGeometry, kind: SaveKind) {
        if !self.can_save() {
            return;
        }
        let revision = self.revision().current;
        let tracked_target = if kind == SaveKind::AutoDraft {
            self.saving
                .copy
                .as_ref()
                .and_then(|(path, fingerprint)| fingerprint.map(|f| (path.clone(), f)))
        } else {
            self.saved_target.clone()
        };
        let snapshot = SaveSnapshot {
            document: self.loaded_request,
            revision,
            strokes: self.edits.snapshot(),
            draft_page,
            appended_pages: self.edits.appended_pages.clone(),
            reference_lines: self.reference_layer(),
            document_id: self.document_id,
            path: path.clone(),
            require_new_target: kind == SaveKind::AutoDraft && tracked_target.is_none(),
            tracked_target,
        };
        self.saving.kind = kind;
        self.saving.reference_lines = Some(snapshot.reference_lines);
        self.saving.destination = Some(path);
        if kind == SaveKind::Manual {
            self.notice = None;
        }
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
                // Use the actual snapshot even if defaults changed during saving.
                self.reference_metadata = self.saving.reference_lines;
                self.saving.paused = false;
                self.notice = None;
                if self.saving.kind == SaveKind::AutoDraft {
                    self.saving.copy = Some((saved.path.clone(), Some(saved.fingerprint)));
                    self.saving.copied_revision = response.revision;
                    self.message = Some(format!(
                        "草稿副本已自动保存 · 修订 {} · Ctrl+S 选择正式路径",
                        response.revision
                    ));
                } else {
                    self.saved_target = Some((saved.path.clone(), saved.fingerprint));
                    self.edits.mark_saved(response.revision);
                    self.saving.restored = false;
                    self.discard_recovery();
                    self.edit_changed();
                    self.message = Some(format!(
                        "{} {} · 修订 {}{}",
                        if self.saving.kind == SaveKind::Manual {
                            "已保存"
                        } else {
                            "已自动保存"
                        },
                        saved.path.display(),
                        response.revision,
                        if self.is_dirty() {
                            " · 仍有新的未保存更改"
                        } else {
                            ""
                        }
                    ));
                }
            }
            Err(error) => {
                // A conflict at a failed Save As destination must not pause
                // autosaving the still-current, unchanged original target.
                let current_target = self.saving.kind != SaveKind::Manual
                    || self.saved_target.as_ref().is_some_and(|(path, _)| {
                        self.saving.destination.as_ref().is_some_and(|destination| {
                            crate::storage::same_file_path(path, destination)
                        })
                    });
                self.saving.paused |= error.conflict && current_target;
                self.set_error(format!(
                    "{}失败：{error}。可按 Ctrl+S 重试或 Ctrl+Shift+S 另存为。",
                    if self.saving.kind == SaveKind::Manual {
                        "保存"
                    } else {
                        "自动保存"
                    }
                ));
            }
        }
    }

    pub fn remove_draft(&self, path: PathBuf) {
        self.worker.remove_draft(path);
    }
    pub fn discard_recovery(&mut self) {
        self.discard_recovery_except(None);
    }

    pub(super) fn discard_recovery_except(&mut self, keep: Option<&Path>) {
        if let Some((path, _)) = self.saving.copy.take()
            && keep != Some(path.as_path())
        {
            self.worker.remove_draft(path);
        }
    }
}
