//! Continuous-document view policy, navigation and page-local hit testing.

mod scroll;
#[cfg(test)]
mod tests;

use crate::document::{
    CanvasViewport, PageGeometry, PageLayout, Point, Rect, Rotation, ViewTransform,
};
use crate::render::PageScene;

use super::area::CanvasResponse;

pub(super) enum ViewAction {
    Fit,
    FitWidth,
    ActualSize,
    Zoom(f64),
    Rotate,
    PreviousPage,
    NextPage,
    GoToPage(u32),
}

/// Scroll distances in logical pixels, measured from the top/left pan limit.
#[derive(Clone, Copy, Debug)]
pub(super) struct ScrollAxis {
    pub offset: f64,
    pub maximum: f64,
    pub visible: f64,
}

pub(super) struct CanvasView {
    layout: PageLayout,
    original_pages: Vec<PageGeometry>,
    current: usize,
    pub transform: ViewTransform,
    pub viewport: Option<CanvasViewport>,
    pub fit_to_page: bool,
    fit_width: bool,
}

impl Default for CanvasView {
    fn default() -> Self {
        Self::new(vec![
            PageGeometry::new(Rect::new(20.0, 30.0, 615.0, 872.0), Rotation::R0, 1.0)
                .expect("valid built-in test page"),
        ])
    }
}

impl CanvasView {
    pub fn new(pages: Vec<PageGeometry>) -> Self {
        Self {
            layout: PageLayout::new(&pages),
            original_pages: pages,
            current: 0,
            transform: ViewTransform::default(),
            viewport: None,
            fit_to_page: true,
            fit_width: false,
        }
    }

    pub fn page(&self) -> PageGeometry {
        self.layout.pages()[self.current].geometry
    }

    pub fn current_page(&self) -> u32 {
        self.current as u32
    }

    pub fn page_count(&self) -> u32 {
        self.layout.pages().len() as u32
    }

    fn fit(&mut self, viewport: Rect) {
        let page = self.layout.pages()[self.current];
        if self.fit_width {
            let zoom =
                (viewport.size().width - 48.0).max(1.0) / page.bounds.size().width / (96.0 / 72.0);
            self.transform.set_zoom_at(zoom, viewport.center());
            self.position_page(viewport, false);
        } else {
            self.transform
                .fit_page(viewport, page.geometry.paper_size(), 24.0);
            self.transform.pan(Point::new(
                -page.bounds.min.x * self.transform.scale(),
                -page.bounds.min.y * self.transform.scale(),
            ));
        }
    }

    fn position_page(&mut self, viewport: Rect, center: bool) {
        let page = self.layout.pages()[self.current];
        let source = if center {
            page.bounds.center()
        } else {
            Point::new(page.bounds.center().x, page.bounds.min.y)
        };
        let target = if center {
            viewport.center()
        } else {
            Point::new(viewport.center().x, viewport.min.y + 24.0)
        };
        let logical = self.transform.paper_to_logical(source);
        self.transform
            .pan(Point::new(target.x - logical.x, target.y - logical.y));
    }

    pub fn update_viewport(&mut self, viewport: CanvasViewport) {
        if self.viewport.map(|old| old.logical) != Some(viewport.logical) {
            if self.fit_to_page || self.fit_width || self.viewport.is_none() {
                self.fit(viewport.logical);
            } else if let Some(old) = self.viewport {
                self.transform.reframe(old.logical, viewport.logical);
            }
        }
        self.viewport = Some(viewport);
    }

    pub fn apply_action(&mut self, action: ViewAction, viewport: Rect) {
        match action {
            ViewAction::Fit | ViewAction::FitWidth => {
                self.fit_to_page = matches!(action, ViewAction::Fit);
                self.fit_width = matches!(action, ViewAction::FitWidth);
                self.fit(viewport);
            }
            ViewAction::ActualSize | ViewAction::Zoom(_) => {
                let zoom = match action {
                    ViewAction::Zoom(factor) => self.transform.zoom() * factor,
                    _ => 1.0,
                };
                self.fit_to_page = false;
                self.fit_width = false;
                self.transform.set_zoom_at(zoom, viewport.center());
            }
            ViewAction::Rotate => {
                let pages: Vec<_> = self
                    .layout
                    .pages()
                    .iter()
                    .map(|page| page.geometry.with_rotation(page.geometry.rotation().next()))
                    .collect();
                self.layout = PageLayout::new(&pages);
                self.fit_to_page = true;
                self.fit_width = false;
                self.fit(viewport);
            }
            ViewAction::PreviousPage | ViewAction::NextPage | ViewAction::GoToPage(_) => {
                self.current = match action {
                    ViewAction::PreviousPage => self.current.saturating_sub(1),
                    ViewAction::NextPage => (self.current + 1).min(self.layout.pages().len() - 1),
                    ViewAction::GoToPage(index) => {
                        (index as usize).min(self.layout.pages().len() - 1)
                    }
                    _ => unreachable!(),
                };
                if self.fit_to_page || self.fit_width {
                    self.fit(viewport);
                } else {
                    self.position_page(viewport, false);
                }
            }
        }
    }

    pub fn apply_interaction(&mut self, response: CanvasResponse) {
        let moved = response.drag_delta.is_some() || response.scroll_delta.is_some();
        if let Some(delta) = response.drag_delta {
            self.transform.pan(delta);
        }
        if let Some(zoom) = &response.zoom {
            self.transform
                .set_zoom_at(self.transform.zoom() * zoom.factor, zoom.anchor);
        }
        if let Some(delta) = response.scroll_delta {
            self.transform.pan(delta);
        }
        if moved {
            self.constrain_pan(response.viewport.logical);
        }
        if moved || response.zoom.is_some() {
            self.fit_to_page = false;
            self.fit_width = false;
            self.current = self.layout.nearest(
                self.transform
                    .logical_to_paper(response.viewport.logical.center()),
            );
        }
    }

    pub fn scenes(&self, viewport: CanvasViewport, test_page: bool) -> Vec<PageScene> {
        let area = Rect {
            min: self.transform.logical_to_paper(viewport.logical.min),
            max: self.transform.logical_to_paper(viewport.logical.max),
        };
        self.layout
            .visible(area)
            .map(|page| PageScene {
                index: page.index,
                page: page.geometry,
                source_page: self.original_pages[page.index as usize],
                view: self.transform.for_page(page.bounds.min),
                bitmap: None,
                tiles: Vec::new(),
                demo_stroke: None,
                strokes: Vec::new(),
                active_stroke: None,
                test_page,
            })
            .collect()
    }

    pub fn pointer_coordinates(&self, logical: Point) -> Option<(u32, Point, Point)> {
        if !self.viewport?.logical.contains(logical) {
            return None;
        }
        let document = self.transform.logical_to_paper(logical);
        let page = self.layout.hit_test(document)?;
        let paper = Point::new(
            document.x - page.bounds.min.x,
            document.y - page.bounds.min.y,
        );
        Some((page.index, paper, page.geometry.paper_to_pdf(paper)))
    }

    pub fn stroke_target(&self, physical: Point) -> Option<crate::ink::StrokeTarget> {
        let viewport = self.viewport?;
        let logical = viewport.physical_to_logical(physical);
        let (index, _, _) = self.pointer_coordinates(logical)?;
        let page = self.layout.pages()[index as usize];
        Some(crate::ink::StrokeTarget {
            page_index: index,
            original: self.original_pages[index as usize],
            displayed: page.geometry,
            view: self.transform.for_page(page.bounds.min),
            viewport,
        })
    }
}
