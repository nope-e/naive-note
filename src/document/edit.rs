//! Session edits over an immutable PDF base. History shares immutable strokes.

use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

use uuid::Uuid;

use super::{InkTransform, PageGeometry, ReferenceLayer, StrokeRecord};

#[derive(Clone, Debug)]
pub enum EditCommand {
    AppendPage(PageGeometry),
    AddStroke(Arc<StrokeRecord>),
    RemoveStrokes(Vec<RemovedStroke>),
    TransformStrokes(Vec<TransformedStroke>),
    ReferenceLines {
        before: Option<ReferenceLayer>,
        after: ReferenceLayer,
    },
}

#[derive(Clone, Debug)]
pub struct RemovedStroke {
    index: usize,
    stroke: Arc<StrokeRecord>,
}

#[derive(Clone, Debug)]
pub struct TransformedStroke {
    before_index: usize,
    before: Arc<StrokeRecord>,
    after_index: usize,
    after: Arc<StrokeRecord>,
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
    pub appended_pages: Vec<PageGeometry>,
    strokes: BTreeMap<u32, Vec<Arc<StrokeRecord>>>,
    stroke_ids: HashSet<Uuid>,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    revision: DocumentRevision,
    last_revision: u64,
    pub reference_lines: Option<ReferenceLayer>,
}

impl EditHistory {
    pub fn contains_stroke(&self, id: Uuid) -> bool {
        self.stroke_ids.contains(&id)
    }
    /// Verified saved ink is an initial content state, not undoable additions.
    pub fn from_strokes(strokes: impl IntoIterator<Item = Arc<StrokeRecord>>) -> Self {
        let mut history = Self::default();
        for stroke in strokes {
            history.apply(&EditCommand::AddStroke(stroke));
        }
        history
    }

    /// Capture original per-page positions so a batch undo preserves stacking.
    pub fn remove_strokes(&mut self, ids: &HashSet<Uuid>) -> bool {
        let removed = self
            .strokes
            .values()
            .flat_map(|page| {
                page.iter()
                    .enumerate()
                    .filter(|(_, stroke)| ids.contains(&stroke.id))
                    .map(|(index, stroke)| RemovedStroke {
                        index,
                        stroke: Arc::clone(stroke),
                    })
            })
            .collect::<Vec<_>>();
        !removed.is_empty() && self.execute(EditCommand::RemoveStrokes(removed))
    }

    pub fn revision(&self) -> DocumentRevision {
        self.revision
    }

    /// One gesture is one history entry; replacements keep UUIDs and stacking.
    pub fn transform_strokes(&mut self, ids: &HashSet<Uuid>, transform: InkTransform) -> bool {
        self.edit_strokes(ids, transform, None)
    }

    pub fn move_strokes(
        &mut self,
        ids: &HashSet<Uuid>,
        transform: InkTransform,
        destination: u32,
    ) -> bool {
        self.edit_strokes(ids, transform, Some(destination))
    }

    fn edit_strokes(
        &mut self,
        ids: &HashSet<Uuid>,
        transform: InkTransform,
        destination: Option<u32>,
    ) -> bool {
        if !transform.is_finite() {
            return false;
        }
        let mut incoming = 0;
        let changed = self
            .strokes
            .values()
            .flat_map(|page| page.iter().enumerate())
            .filter(|(_, stroke)| {
                ids.contains(&stroke.id)
                    && (!transform.is_identity()
                        || destination.is_some_and(|page| page != stroke.page_index))
            })
            .map(|(index, stroke)| {
                let page = destination.unwrap_or(stroke.page_index);
                let after_index = if page == stroke.page_index {
                    index
                } else {
                    let index = self.strokes(page).len() + incoming;
                    incoming += 1;
                    index
                };
                let mut after = stroke.transformed(transform);
                after.page_index = page;
                TransformedStroke {
                    before_index: index,
                    before: Arc::clone(stroke),
                    after_index,
                    after: Arc::new(after),
                }
            })
            .collect::<Vec<_>>();
        !changed.is_empty() && self.execute(EditCommand::TransformStrokes(changed))
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
            EditCommand::RemoveStrokes(strokes) if strokes.is_empty() => return false,
            EditCommand::TransformStrokes(strokes) if strokes.is_empty() => return false,
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
            EditCommand::AppendPage(_) => {
                self.appended_pages.pop().expect("history page");
            }
            EditCommand::RemoveStrokes(strokes) => {
                for removed in strokes {
                    self.stroke_ids.insert(removed.stroke.id);
                    self.strokes
                        .entry(removed.stroke.page_index)
                        .or_default()
                        .insert(removed.index, Arc::clone(&removed.stroke));
                }
            }
            EditCommand::ReferenceLines { before, .. } => self.reference_lines = *before,
            EditCommand::TransformStrokes(strokes) => self.replace_strokes(strokes, false),
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
            EditCommand::AppendPage(page) => self.appended_pages.push(*page),
            EditCommand::RemoveStrokes(strokes) => {
                let ids: HashSet<_> = strokes.iter().map(|s| s.stroke.id).collect();
                self.strokes.retain(|_, page| {
                    page.retain(|stroke| !ids.contains(&stroke.id));
                    !page.is_empty()
                });
                self.stroke_ids.retain(|id| !ids.contains(id));
            }
            EditCommand::ReferenceLines { after, .. } => self.reference_lines = Some(*after),
            EditCommand::TransformStrokes(strokes) => self.replace_strokes(strokes, true),
            EditCommand::AddStroke(stroke) => {
                self.stroke_ids.insert(stroke.id);
                self.strokes
                    .entry(stroke.page_index)
                    .or_default()
                    .push(Arc::clone(stroke));
            }
        }
    }

    fn replace_strokes(&mut self, changed: &[TransformedStroke], forward: bool) {
        // Remove first, then restore exact per-page slots in ascending order.
        // Transfers append above target ink; undo restores both pages' stacking.
        let ids: HashSet<_> = changed.iter().map(|s| s.before.id).collect();
        self.strokes.retain(|_, page| {
            page.retain(|s| !ids.contains(&s.id));
            !page.is_empty()
        });
        let mut replacements = changed
            .iter()
            .map(|s| {
                if forward {
                    (s.after_index, &s.after)
                } else {
                    (s.before_index, &s.before)
                }
            })
            .collect::<Vec<_>>();
        replacements.sort_by_key(|(index, stroke)| (stroke.page_index, *index));
        for (index, stroke) in replacements {
            self.strokes
                .entry(stroke.page_index)
                .or_default()
                .insert(index, Arc::clone(stroke));
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
