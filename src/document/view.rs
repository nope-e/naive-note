use super::{Point, Rect, Size};

/// At 100%, one 1/72-inch paper point occupies 96/72 Windows logical pixels.
const LOGICAL_PIXELS_PER_POINT: f64 = 96.0 / 72.0;
pub const MIN_ZOOM: f64 = 0.1;
pub const MAX_ZOOM: f64 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasViewport {
    pub logical: Rect,
    pub physical: PixelRect,
    pub pixels_per_point: f64,
}

impl CanvasViewport {
    /// Physical bounds enclose the logical rect and are clamped to the framebuffer.
    /// The renderer still clips to the exact fractional logical edges.
    pub fn new(logical: Rect, pixels_per_point: f64, framebuffer: (u32, u32)) -> Option<Self> {
        if !logical.is_valid() || !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
            return None;
        }
        let physical = PixelRect {
            left: (logical.min.x * pixels_per_point)
                .floor()
                .clamp(0.0, f64::from(framebuffer.0)) as u32,
            top: (logical.min.y * pixels_per_point)
                .floor()
                .clamp(0.0, f64::from(framebuffer.1)) as u32,
            right: (logical.max.x * pixels_per_point)
                .ceil()
                .clamp(0.0, f64::from(framebuffer.0)) as u32,
            bottom: (logical.max.y * pixels_per_point)
                .ceil()
                .clamp(0.0, f64::from(framebuffer.1)) as u32,
        };
        (physical.right > physical.left && physical.bottom > physical.top).then_some(Self {
            logical,
            physical,
            pixels_per_point,
        })
    }

    pub fn logical_to_physical(self, point: Point) -> Point {
        Point::new(
            point.x * self.pixels_per_point,
            point.y * self.pixels_per_point,
        )
    }

    pub fn physical_to_logical(self, point: Point) -> Point {
        Point::new(
            point.x / self.pixels_per_point,
            point.y / self.pixels_per_point,
        )
    }
}

/// Paper points -> client-area logical coordinates. DPI is a separate final step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewTransform {
    origin: Point,
    zoom: f64,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self {
            origin: Point::new(0.0, 0.0),
            zoom: 1.0,
        }
    }
}

impl ViewTransform {
    pub fn zoom(self) -> f64 {
        self.zoom
    }

    pub fn scale(self) -> f64 {
        self.zoom * LOGICAL_PIXELS_PER_POINT
    }

    pub fn paper_to_logical(self, paper: Point) -> Point {
        Point::new(
            self.origin.x + paper.x * self.scale(),
            self.origin.y + paper.y * self.scale(),
        )
    }

    pub fn logical_to_paper(self, logical: Point) -> Point {
        Point::new(
            (logical.x - self.origin.x) / self.scale(),
            (logical.y - self.origin.y) / self.scale(),
        )
    }

    pub fn pan(&mut self, delta: Point) {
        if delta.is_finite() {
            let origin = Point::new(self.origin.x + delta.x, self.origin.y + delta.y);
            if origin.is_finite() {
                self.origin = origin;
            }
        }
    }

    /// Preserve the paper point under an arbitrary client-area anchor, including
    /// when zoom is clamped at either end of the supported range.
    pub fn set_zoom_at(&mut self, zoom: f64, anchor: Point) {
        if !zoom.is_finite() || zoom <= 0.0 || !anchor.is_finite() {
            return;
        }
        let paper = self.logical_to_paper(anchor);
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.origin = Point::new(
            anchor.x - paper.x * self.scale(),
            anchor.y - paper.y * self.scale(),
        );
    }

    pub fn fit_page(&mut self, viewport: Rect, page: Size, padding: f64) {
        if !viewport.is_valid() || !page.is_valid() || !padding.is_finite() || padding < 0.0 {
            return;
        }
        let available = viewport.size();
        let scale = ((available.width - padding * 2.0).max(1.0) / page.width)
            .min((available.height - padding * 2.0).max(1.0) / page.height);
        self.zoom = (scale / LOGICAL_PIXELS_PER_POINT).clamp(MIN_ZOOM, MAX_ZOOM);
        let center = viewport.center();
        self.origin = Point::new(
            center.x - page.width * self.scale() * 0.5,
            center.y - page.height * self.scale() * 0.5,
        );
    }

    /// Retain the paper position at the canvas center during resize/layout changes.
    pub fn reframe(&mut self, old: Rect, new: Rect) {
        let old_center = old.center();
        let new_center = new.center();
        self.pan(Point::new(
            new_center.x - old_center.x,
            new_center.y - old_center.y,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{PageGeometry, Rotation};

    fn near(a: Point, b: Point) {
        assert!(
            (a.x - b.x).abs() < 1e-8 && (a.y - b.y).abs() < 1e-8,
            "{a:?} != {b:?}"
        );
    }

    #[test]
    fn hundred_percent_maps_paper_inches_to_windows_logical_pixels() {
        let view = ViewTransform::default();
        let paper = Point::new(72.0, 144.0);
        near(view.paper_to_logical(paper), Point::new(96.0, 192.0));
        for dpi in [1.0, 1.5, 2.0] {
            let viewport = CanvasViewport::new(
                Rect::new(0.0, 0.0, 800.0, 600.0),
                dpi,
                ((800.0 * dpi) as u32, (600.0 * dpi) as u32),
            )
            .unwrap();
            near(
                viewport.logical_to_physical(view.paper_to_logical(paper)),
                Point::new(96.0 * dpi, 192.0 * dpi),
            );
        }
    }

    #[test]
    fn pointer_anchor_survives_zoom_and_limits() {
        let mut view = ViewTransform::default();
        view.pan(Point::new(-217.5, 81.25));
        let pointer = Point::new(350.5, 421.75);
        let paper = view.logical_to_paper(pointer);
        for zoom in [2.5, 0.25, 100.0, 0.001, 1.0] {
            view.set_zoom_at(zoom, pointer);
            near(view.paper_to_logical(paper), pointer);
            near(view.logical_to_paper(pointer), paper);
            assert!((MIN_ZOOM..=MAX_ZOOM).contains(&view.zoom()));
        }
    }

    #[test]
    fn fit_is_centered_and_keeps_padding() {
        let viewport = Rect::new(0.0, 48.0, 1000.0, 740.0);
        let page = Size::new(595.0, 842.0);
        let mut view = ViewTransform::default();
        view.fit_page(viewport, page, 24.0);
        near(
            view.paper_to_logical(Point::new(page.width * 0.5, page.height * 0.5)),
            viewport.center(),
        );
        let top_left = view.paper_to_logical(Point::default());
        let bottom_right = view.paper_to_logical(Point::new(page.width, page.height));
        assert!(top_left.x >= viewport.min.x + 24.0 - 1e-8);
        assert!(top_left.y >= viewport.min.y + 24.0 - 1e-8);
        assert!(bottom_right.x <= viewport.max.x - 24.0 + 1e-8);
        assert!(bottom_right.y <= viewport.max.y - 24.0 + 1e-8);
    }

    #[test]
    fn resize_retains_center_and_pan_uses_logical_pixels() {
        let old = Rect::new(0.0, 40.0, 1000.0, 720.0);
        let new = Rect::new(0.0, 80.0, 700.0, 500.0);
        let mut view = ViewTransform::default();
        let paper = view.logical_to_paper(old.center());
        view.reframe(old, new);
        near(view.paper_to_logical(paper), new.center());
        view.pan(Point::new(30.0, -45.0));
        near(
            view.paper_to_logical(paper),
            Point::new(new.center().x + 30.0, new.center().y - 45.0),
        );
    }

    #[test]
    fn pdf_paper_logical_physical_chain_round_trips_at_all_dpis() {
        for dpi in [1.0, 1.5, 2.0] {
            let viewport = CanvasViewport::new(
                Rect::new(0.0, 40.0, 800.0, 580.0),
                dpi,
                ((800.0 * dpi) as u32, (600.0 * dpi) as u32),
            )
            .unwrap();
            for rotation in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
                let page = PageGeometry::new(Rect::new(-20.0, 30.0, 456.0, 703.6), rotation, 1.25)
                    .unwrap();
                let mut view = ViewTransform::default();
                view.fit_page(viewport.logical, page.paper_size(), 24.0);
                view.set_zoom_at(1.75, viewport.logical.center());
                let pdf = Point::new(82.25, 456.5);
                let paper = page.pdf_to_paper(pdf);
                let physical = viewport.logical_to_physical(view.paper_to_logical(paper));
                near(
                    page.paper_to_pdf(
                        view.logical_to_paper(viewport.physical_to_logical(physical)),
                    ),
                    pdf,
                );
            }
        }
    }

    #[test]
    fn viewport_pixel_bounds_round_outward_and_clamp_to_framebuffer() {
        let viewport =
            CanvasViewport::new(Rect::new(-2.0, 10.25, 200.75, 100.25), 1.5, (300, 160)).unwrap();
        assert_eq!(
            viewport.physical,
            PixelRect {
                left: 0,
                top: 15,
                right: 300,
                bottom: 151
            }
        );
        assert!(CanvasViewport::new(Rect::new(0.0, 0.0, 0.0, 10.0), 1.0, (100, 100)).is_none());
        assert!(
            CanvasViewport::new(Rect::new(0.0, 0.0, 10.0, 10.0), f64::NAN, (100, 100)).is_none()
        );
        assert!(CanvasViewport::new(Rect::new(0.0, 0.0, 10.0, 10.0), 1.0, (0, 0)).is_none());
    }

    #[test]
    fn non_finite_view_inputs_do_not_corrupt_transform() {
        let mut view = ViewTransform::default();
        let before = view;
        view.pan(Point::new(f64::NAN, 0.0));
        view.set_zoom_at(f64::INFINITY, Point::default());
        view.set_zoom_at(2.0, Point::new(f64::NAN, 0.0));
        view.fit_page(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Size::new(0.0, 100.0),
            24.0,
        );
        assert_eq!(view, before);
    }
}
