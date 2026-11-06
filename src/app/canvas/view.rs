//! Continuous-document view policy, navigation and page-local hit testing.

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

pub(super) struct CanvasView {
    layout: PageLayout,
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

    fn constrain_pan(&mut self, viewport: Rect) {
        let size = self.layout.size();
        let scale = self.transform.scale();
        let origin = self.transform.paper_to_logical(Point::default());
        let width = size.width * scale;
        let x = if width <= viewport.size().width - 48.0 {
            viewport.center().x - width * 0.5
        } else {
            origin
                .x
                .clamp(viewport.max.x - 24.0 - width, viewport.min.x + 24.0)
        };
        let pages = self.layout.pages();
        // Allow the first and last page to reach the viewport center, while
        // preventing scrolling indefinitely beyond the document.
        let first =
            (viewport.center().y - pages[0].bounds.center().y * scale).max(viewport.min.y + 24.0);
        let last = (viewport.center().y - pages.last().unwrap().bounds.center().y * scale)
            .min(viewport.max.y - 24.0 - size.height * scale);
        let y = origin.y.clamp(last, first);
        self.transform.pan(Point::new(x - origin.x, y - origin.y));
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
                view: self.transform.for_page(page.bounds.min),
                bitmap: None,
                tiles: Vec::new(),
                demo_stroke: None,
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
}

#[cfg(test)]
mod tests {
    use super::super::area::ZoomGesture;
    use super::*;

    fn setup() -> (CanvasView, CanvasViewport) {
        let mut view = CanvasView::new(vec![
            PageGeometry::new(Rect::new(20.0, 30.0, 220.0, 330.0), Rotation::R0, 1.5).unwrap(),
            PageGeometry::new(Rect::new(-10.0, 40.0, 390.0, 240.0), Rotation::R90, 2.0).unwrap(),
        ]);
        let viewport =
            CanvasViewport::new(Rect::new(0.0, 40.0, 800.0, 640.0), 1.5, (1200, 960)).unwrap();
        view.update_viewport(viewport);
        (view, viewport)
    }

    fn near(actual: Point, expected: Point) {
        assert!(
            (actual.x - expected.x).abs() < 1e-8 && (actual.y - expected.y).abs() < 1e-8,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn navigation_fits_target_page_and_maps_pointer_to_its_own_pdf_space() {
        let (mut view, viewport) = setup();
        view.apply_action(ViewAction::NextPage, viewport.logical);
        assert_eq!(view.current_page(), 1);
        let (index, paper, pdf) = view.pointer_coordinates(viewport.logical.center()).unwrap();
        assert_eq!(index, 1);
        near(paper, Point::new(200.0, 400.0));
        near(view.page().pdf_to_paper(pdf), paper);
        let scene = view
            .scenes(viewport, false)
            .into_iter()
            .find(|page| page.index == 1)
            .unwrap();
        near(
            scene.view.paper_to_logical(paper),
            viewport.logical.center(),
        );
        view.apply_action(ViewAction::GoToPage(u32::MAX), viewport.logical);
        assert_eq!(view.current_page(), 1);
        view.apply_action(ViewAction::PreviousPage, viewport.logical);
        view.apply_action(ViewAction::PreviousPage, viewport.logical);
        assert_eq!(view.current_page(), 0);
    }

    #[test]
    fn scroll_tracks_current_page_and_clamps_at_document_ends() {
        let (mut view, viewport) = setup();
        for delta in [-100000.0, 100000.0] {
            view.apply_interaction(CanvasResponse {
                viewport,
                drag_delta: None,
                zoom: None,
                scroll_delta: Some(Point::new(0.0, delta)),
            });
            assert_eq!(view.current_page(), u32::from(delta < 0.0));
            assert!(!view.scenes(viewport, false).is_empty());
        }
    }

    #[test]
    fn zoom_keeps_pointer_anchor_and_rotation_preserves_target_page() {
        let (mut view, viewport) = setup();
        view.apply_action(ViewAction::NextPage, viewport.logical);
        let anchor = Point::new(380.0, 360.0);
        let before = view.pointer_coordinates(anchor).unwrap();
        view.apply_interaction(CanvasResponse {
            viewport,
            drag_delta: None,
            zoom: Some(ZoomGesture {
                factor: 2.0,
                anchor,
            }),
            scroll_delta: None,
        });
        let after = view.pointer_coordinates(anchor).unwrap();
        assert_eq!(before.0, after.0);
        near(before.2, after.2);
        let zoom = view.transform.zoom();
        view.apply_action(ViewAction::GoToPage(0), viewport.logical);
        assert_eq!(view.transform.zoom(), zoom);
        view.apply_action(ViewAction::NextPage, viewport.logical);
        let geometry = view.page();
        for _ in 0..4 {
            view.apply_action(ViewAction::Rotate, viewport.logical);
        }
        assert_eq!(view.current_page(), 1);
        assert_eq!(view.page(), geometry);
    }

    #[test]
    fn fit_width_and_resize_use_the_active_page() {
        let (mut view, viewport) = setup();
        view.apply_action(ViewAction::NextPage, viewport.logical);
        view.apply_action(ViewAction::FitWidth, viewport.logical);
        let scene = view
            .scenes(viewport, false)
            .into_iter()
            .find(|page| page.index == 1)
            .unwrap();
        near(
            scene.view.paper_to_logical(Point::default()),
            Point::new(24.0, 64.0),
        );
        let smaller =
            CanvasViewport::new(Rect::new(0.0, 60.0, 600.0, 600.0), 2.0, (1200, 1200)).unwrap();
        view.update_viewport(smaller);
        let scene = view
            .scenes(smaller, false)
            .into_iter()
            .find(|page| page.index == 1)
            .unwrap();
        near(
            scene.view.paper_to_logical(Point::default()),
            Point::new(24.0, 84.0),
        );
    }
}
