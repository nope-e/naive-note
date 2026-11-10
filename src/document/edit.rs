//! Session edits over an immutable PDF base. History shares immutable strokes.

use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

use uuid::Uuid;

use super::{ReferenceLayer, StrokeRecord};

#[derive(Clone, Debug)]
pub enum EditCommand {
    AddStroke(Arc<StrokeRecord>),
    ReferenceLines {
        before: Option<ReferenceLayer>,
        after: ReferenceLayer,
    },
}

/// Identities name content states, not history depths. Undo/redo return to the
/// same identity; a new branch always receives a fresh one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DocumentRevision {
    pub current: u64,
    pub saved: u64,
}

impl DocumentRevision {
    pub fn is_dirty(self) -> bool {
        self.current != self.saved
    }
}

struct HistoryEntry {
    command: EditCommand,
    before: u64,
    after: u64,
}

#[derive(Default)]
pub struct EditHistory {
    strokes: BTreeMap<u32, Vec<Arc<StrokeRecord>>>,
    stroke_ids: HashSet<Uuid>,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    revision: DocumentRevision,
    last_revision: u64,
    pub reference_lines: Option<ReferenceLayer>,
}

impl EditHistory {
    pub fn revision(&self) -> DocumentRevision {
        self.revision
    }

    pub fn strokes(&self, page: u32) -> &[Arc<StrokeRecord>] {
        self.strokes.get(&page).map_or(&[], Vec::as_slice)
    }

    pub fn stroke_count(&self) -> usize {
        self.stroke_ids.len()
    }

    pub fn snapshot(&self) -> Vec<Arc<StrokeRecord>> {
        self.strokes.values().flatten().cloned().collect()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// A repeated submission cannot duplicate ink or destroy the redo branch.
    pub fn execute(&mut self, command: EditCommand) -> bool {
        match &command {
            EditCommand::AddStroke(stroke) if self.stroke_ids.contains(&stroke.id) => return false,
            EditCommand::ReferenceLines { after, .. } if self.reference_lines == Some(*after) => {
                return false;
            }
            _ => {}
        }
        let next = self
            .last_revision
            .checked_add(1)
            .expect("revision exhausted");
        self.apply(&command);
        self.undo.push(HistoryEntry {
            command,
            before: self.revision.current,
            after: next,
        });
        self.redo.clear();
        self.last_revision = next;
        self.revision.current = next;
        true
    }

    pub fn undo(&mut self) -> bool {
        let Some(entry) = self.undo.pop() else {
            return false;
        };
        match &entry.command {
            EditCommand::ReferenceLines { before, .. } => self.reference_lines = *before,
            EditCommand::AddStroke(stroke) => {
                let page = self
                    .strokes
                    .get_mut(&stroke.page_index)
                    .expect("stroke page");
                // History order makes this the last stroke on its page even
                // when intervening commands changed reference lines or other pages.
                let removed = page.pop().expect("history stroke");
                debug_assert_eq!(removed.id, stroke.id);
                if page.is_empty() {
                    self.strokes.remove(&stroke.page_index);
                }
                self.stroke_ids.remove(&stroke.id);
            }
        }
        self.revision.current = entry.before;
        self.redo.push(entry);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(entry) = self.redo.pop() else {
            return false;
        };
        self.apply(&entry.command);
        self.revision.current = entry.after;
        self.undo.push(entry);
        true
    }

    fn apply(&mut self, command: &EditCommand) {
        match command {
            EditCommand::ReferenceLines { after, .. } => self.reference_lines = Some(*after),
            EditCommand::AddStroke(stroke) => {
                self.stroke_ids.insert(stroke.id);
                self.strokes
                    .entry(stroke.page_index)
                    .or_default()
                    .push(Arc::clone(stroke));
            }
        }
    }

    /// Step 8 calls this only after the snapshot's save transaction succeeds.
    /// Do not clear history or substitute the current revision: edits may have
    /// continued since the saved snapshot was captured.
    pub fn mark_saved(&mut self, revision: u64) {
        self.revision.saved = revision;
    }
}

#[cfg(test)]
mod tests;
