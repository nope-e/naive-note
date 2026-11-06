//! Canvas view policy: fitting, navigation and pointer-coordinate conversion.
//! Coordinate mathematics itself lives in `crate::document`.

use crate::document::{CanvasViewport, PageGeometry, Point, Rect, Rotation, ViewTransform};

use super::area::CanvasResponse;

pub(super) enum ViewAction {
    Fit,
    ActualSize,
    Zoom(f64),
    Rotate,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct CanvasView {
    pub page: PageGeometry,
    pub transform: ViewTransform,
    pub viewport: Option<CanvasViewport>,
    pub fit_to_page: bool,
}

impl Default for CanvasView {
    fn default() -> Self {
        // Synthetic A4-sized CropBox, deliberately offset in PDF user space.
        Self::new(
            PageGeometry::new(Rect::new(20.0, 30.0, 615.0, 872.0), Rotation::R0, 1.0)
                .expect("valid built-in test page"),
        )
    }
}

impl CanvasView {
    pub fn new(page: PageGeometry) -> Self {
        Self {
            page,
            transform: ViewTransform::default(),
            viewport: None,
            fit_to_page: true,
        }
    }

    pub fn update_viewport(&mut self, viewport: CanvasViewport) {
        if self.viewport.map(|old| old.logical) != Some(viewport.logical) {
            if self.fit_to_page || self.viewport.is_none() {
                self.transform
                    .fit_page(viewport.logical, self.page.paper_size(), 24.0);
            } else if let Some(old) = self.viewport {
                self.transform.reframe(old.logical, viewport.logical);
            }
        }
        self.viewport = Some(viewport);
    }

    pub fn apply_action(&mut self, action: ViewAction, viewport: Rect) {
        match action {
            ViewAction::Fit => {
                self.fit_to_page = true;
                self.transform
                    .fit_page(viewport, self.page.paper_size(), 24.0);
            }
            ViewAction::ActualSize => {
                self.fit_to_page = false;
                self.transform.set_zoom_at(1.0, viewport.center());
            }
            ViewAction::Zoom(factor) => {
                self.fit_to_page = false;
                self.transform
                    .set_zoom_at(self.transform.zoom() * factor, viewport.center());
            }
            ViewAction::Rotate => {
                self.page = self.page.with_rotation(self.page.rotation().next());
                self.fit_to_page = true;
                self.transform
                    .fit_page(viewport, self.page.paper_size(), 24.0);
            }
        }
    }

    pub fn apply_interaction(&mut self, response: CanvasResponse) {
        if let Some(delta) = response.drag_delta {
            self.transform.pan(delta);
            self.fit_to_page = false;
        }
        if let Some(zoom) = response.zoom {
            self.transform
                .set_zoom_at(self.transform.zoom() * zoom.factor, zoom.anchor);
            self.fit_to_page = false;
        }
        if let Some(delta) = response.scroll_delta {
            self.transform.pan(delta);
            self.fit_to_page = false;
        }
    }

    pub fn pointer_coordinates(&self, logical: Point) -> Option<(Point, Point)> {
        if !self.viewport?.logical.contains(logical) {
            return None;
        }
        let paper = self.transform.logical_to_paper(logical);
        self.page
            .contains_paper(paper)
            .then(|| (paper, self.page.paper_to_pdf(paper)))
    }
}
