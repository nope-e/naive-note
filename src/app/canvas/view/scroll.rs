//! Shared pan limits and scrollbar offset conversion for a continuous document.

use crate::document::{Point, Rect};

use super::{CanvasView, ScrollAxis};

impl CanvasView {
    /// Shared by wheel/drag constraints and scrollbar offset conversion.
    fn pan_limits(&self, viewport: Rect) -> Rect {
        let size = self.layout.size();
        let scale = self.transform.scale();
        let width = size.width * scale;
        let (left, right) = if width <= viewport.size().width - 48.0 {
            let center = viewport.center().x - width * 0.5;
            (center, center)
        } else {
            (viewport.max.x - 24.0 - width, viewport.min.x + 24.0)
        };
        let pages = self.layout.pages();
        // Allow the first and last page to reach the viewport center, while
        // preventing scrolling indefinitely beyond the document.
        let first =
            (viewport.center().y - pages[0].bounds.center().y * scale).max(viewport.min.y + 24.0);
        let last = (viewport.center().y - pages.last().unwrap().bounds.center().y * scale)
            .min(viewport.max.y - 24.0 - size.height * scale);
        Rect::new(left, last, right, first)
    }

    pub(super) fn constrain_pan(&mut self, viewport: Rect) {
        let limits = self.pan_limits(viewport);
        let origin = self.transform.paper_to_logical(Point::default());
        let x = origin.x.clamp(limits.min.x, limits.max.x);
        let y = origin.y.clamp(limits.min.y, limits.max.y);
        self.transform.pan(Point::new(x - origin.x, y - origin.y));
    }

    pub fn scroll_axes(&self, viewport: Rect) -> [ScrollAxis; 2] {
        let limits = self.pan_limits(viewport);
        let origin = self.transform.paper_to_logical(Point::default());
        let axis = |min: f64, max: f64, origin: f64, visible| {
            let maximum = max - min;
            ScrollAxis {
                offset: (max - origin).clamp(0.0, maximum),
                maximum,
                visible,
            }
        };
        [
            axis(limits.min.x, limits.max.x, origin.x, viewport.size().width),
            axis(limits.min.y, limits.max.y, origin.y, viewport.size().height),
        ]
    }

    pub fn apply_scroll_offsets(&mut self, viewport: Rect, offsets: [Option<f64>; 2]) {
        let [x, y] = offsets.map(|offset| offset.filter(|value| value.is_finite()));
        if x.is_none() && y.is_none() {
            return;
        }
        let limits = self.pan_limits(viewport);
        let origin = self.transform.paper_to_logical(Point::default());
        self.transform.pan(Point::new(
            x.map_or(0.0, |x| {
                limits.max.x - x.clamp(0.0, limits.max.x - limits.min.x) - origin.x
            }),
            y.map_or(0.0, |y| {
                limits.max.y - y.clamp(0.0, limits.max.y - limits.min.y) - origin.y
            }),
        ));
        self.constrain_pan(viewport);
        self.fit_to_page = false;
        self.fit_width = false;
        self.current = self
            .layout
            .nearest(self.transform.logical_to_paper(viewport.center()));
    }
}
