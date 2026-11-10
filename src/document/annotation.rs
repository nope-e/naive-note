//! Standard PDF objects share selection/history with ink without inventing capture data.

use std::sync::Arc;

use super::{Point, Rect, StrokeRecord};

#[derive(Clone, Debug, PartialEq)]
pub struct PdfAnnotationSource {
    pub xref: i32,
    pub page_index: u32,
    pub bounds: Rect,
    pub svg: Arc<str>,
    /// A standalone copy supplies generated AP resources when the original PDF
    /// had no explicit appearance. It is never written into the edit attachment.
    pub appearance_pdf: Arc<[u8]>,
    pub multiply: bool,
}

impl StrokeRecord {
    pub fn annotation_corners(&self) -> Option<[Point; 4]> {
        let bounds = self.pdf_annotation.as_ref()?.bounds;
        Some(
            [
                bounds.min,
                Point::new(bounds.max.x, bounds.min.y),
                bounds.max,
                Point::new(bounds.min.x, bounds.max.y),
            ]
            .map(|point| self.edit_transform.apply(point)),
        )
    }
}

pub(super) fn verified_by_default() -> bool {
    true
}
