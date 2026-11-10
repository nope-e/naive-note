//! Pure paper-space hit tests shared by object selection and the object eraser.

use super::{Point, Rect, StrokeRecord};

impl StrokeRecord {
    pub fn bounds(&self) -> Rect {
        let mut bounds = Rect::new(
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for sample in &self.modeled_samples {
            let r = self.style.width(sample.pressure) * 0.5;
            bounds.min.x = bounds.min.x.min(sample.position.x - r);
            bounds.min.y = bounds.min.y.min(sample.position.y - r);
            bounds.max.x = bounds.max.x.max(sample.position.x + r);
            bounds.max.y = bounds.max.y.max(sample.position.y + r);
        }
        bounds
    }

    /// Swept disk: sparse pointer samples must still erase crossed objects.
    pub fn hit_segment(&self, from: Point, to: Point, radius: f64) -> bool {
        let bounds = self.bounds();
        if from.x.min(to.x) - radius > bounds.max.x
            || from.x.max(to.x) + radius < bounds.min.x
            || from.y.min(to.y) - radius > bounds.max.y
            || from.y.max(to.y) + radius < bounds.min.y
        {
            return false;
        }
        // Match round_outline's disks and tapered connecting trapezoids. Using
        // only a segment's maximum width would erase beside its thin end.
        if self.modeled_samples.iter().any(|s| {
            point_distance(s.position, from, to) <= radius + self.style.width(s.pressure) * 0.5
        }) {
            return true;
        }
        self.modeled_samples.windows(2).any(|pair| {
            let [a, b] = pair else { unreachable!() };
            let dx = b.position.x - a.position.x;
            let dy = b.position.y - a.position.y;
            let length = dx.hypot(dy);
            if length <= f64::EPSILON {
                return false;
            }
            let offset =
                |p: Point, r: f64| Point::new(p.x - dy / length * r, p.y + dx / length * r);
            let ar = self.style.width(a.pressure) * 0.5;
            let br = self.style.width(b.pressure) * 0.5;
            let polygon = [
                offset(a.position, -ar),
                offset(b.position, -br),
                offset(b.position, br),
                offset(a.position, ar),
            ];
            let inside = |p| {
                (0..4).all(|i| cross(polygon[i], polygon[(i + 1) % 4], p) >= 0.0)
                    || (0..4).all(|i| cross(polygon[i], polygon[(i + 1) % 4], p) <= 0.0)
            };
            inside(from)
                || inside(to)
                || (0..4)
                    .any(|i| segment_distance(from, to, polygon[i], polygon[(i + 1) % 4]) <= radius)
        })
    }
}

fn point_distance(p: Point, a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length2 = dx * dx + dy * dy;
    let t = if length2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / length2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}

fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn segment_distance(a: Point, b: Point, c: Point, d: Point) -> f64 {
    // Strict crossing; endpoint/collinear/degenerate cases use point distance.
    let ab = (cross(a, b, c), cross(a, b, d));
    let cd = (cross(c, d, a), cross(c, d, b));
    if ab.0 * ab.1 < 0.0 && cd.0 * cd.1 < 0.0 {
        return 0.0;
    }
    point_distance(a, c, d)
        .min(point_distance(b, c, d))
        .min(point_distance(c, a, b))
        .min(point_distance(d, a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tapered_ink_does_not_hit_empty_space_beside_its_thin_end() {
        use crate::document::{ModeledSample, PageGeometry, Rotation};
        let geometry = PageGeometry::new(Rect::new(0., 0., 600., 800.), Rotation::R0, 1.).unwrap();
        let mut stroke = crate::ink::test_support::pressure_stroke(geometry).unwrap();
        stroke.style.max_width_pt = 40.;
        stroke.style.min_width_ratio = 0.05;
        stroke.style.pressure_gamma = 1.;
        stroke.modeled_samples = vec![
            ModeledSample {
                position: Point::new(0., 0.),
                time_us: 0,
                pressure: 0.,
            },
            ModeledSample {
                position: Point::new(100., 0.),
                time_us: 1,
                pressure: 1.,
            },
        ];
        let empty = Point::new(2., 15.);
        assert!(!stroke.hit_segment(empty, empty, 0.));
        let filled = Point::new(80., 10.);
        assert!(stroke.hit_segment(filled, filled, 0.));
        assert!(stroke.hit_segment(Point::new(50., -40.), Point::new(50., 40.), 0.));
    }

    #[test]
    fn sweep_handles_crossings_endpoints_dots_and_parallel_misses() {
        let p = Point::new;
        assert_eq!(
            segment_distance(p(0., 5.), p(10., 5.), p(5., 0.), p(5., 10.)),
            0.
        );
        assert_eq!(
            segment_distance(p(0., 0.), p(2., 0.), p(4., 0.), p(8., 0.)),
            2.
        );
        assert_eq!(
            segment_distance(p(0., 0.), p(0., 0.), p(3., 4.), p(3., 4.)),
            5.
        );
        assert_eq!(
            segment_distance(p(0., 0.), p(10., 0.), p(0., 3.), p(10., 3.)),
            3.
        );
    }
}
