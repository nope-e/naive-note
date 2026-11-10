//! Extend paper from real drawing samples, independently of redraw cadence.

use crate::document::StrokeRecord;

impl super::CanvasWorkspace {
    pub(super) fn expand_for_ink(&mut self, completed: Option<&StrokeRecord>) {
        if self.canvas_blocked() || !self.document.reference_layer().auto_expand {
            return;
        }
        let sample = self.input.last_ink_sample().or_else(|| {
            completed.and_then(|stroke| {
                stroke
                    .raw_samples
                    .last()
                    .map(|sample| (stroke.page_index, sample.position))
            })
        });
        if let Some((page, position)) = sample
            && self.view.ink_past_page_end(page, position)
            && self.document.append_page(self.view.draft_page())
        {
            self.sync_document_pages();
        }
    }
}
