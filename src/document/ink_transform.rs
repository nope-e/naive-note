//! Rigid edits in source paper coordinates, shared by previews and committed ink.

use super::{PathCommand, Point, StrokeRecord};

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InkTransform {
    pub center: Point,
    pub translation: Point,
    pub radians: f64,
}

impl InkTransform {
    pub fn is_identity(&self) -> bool {
        let angle = self.radians.rem_euclid(std::f64::consts::TAU);
        self.translation.x.hypot(self.translation.y) < 1e-9
            && angle.min(std::f64::consts::TAU - angle) < 1e-12
    }

    pub fn is_finite(self) -> bool {
        self.center.is_finite() && self.translation.is_finite() && self.radians.is_finite()
    }

    pub fn apply(self, point: Point) -> Point {
        let (sin, cos) = self.radians.sin_cos();
        let x = point.x - self.center.x;
        let y = point.y - self.center.y;
        Point::new(
            self.center.x + cos * x - sin * y + self.translation.x,
            self.center.y + sin * x + cos * y + self.translation.y,
        )
    }

    /// Apply this mapping, then the next rigid edit or page-space mapping.
    pub fn followed_by(self, next: Self) -> Self {
        Self {
            center: Point::default(),
            translation: next.apply(self.apply(Point::default())),
            radians: (self.radians + next.radians).rem_euclid(std::f64::consts::TAU),
        }
    }
}

impl StrokeRecord {
    /// Bake displayed geometry while retaining the exact captured samples and
    /// recording their mapping to the edited paper. IDs, style and page stay intact.
    pub fn transformed(&self, transform: InkTransform) -> Self {
        let mut stroke = self.clone();
        stroke.edit_transform = self.edit_transform.followed_by(transform);
        for sample in &mut stroke.modeled_samples {
            sample.position = transform.apply(sample.position);
        }
        for command in &mut stroke.outline {
            match command {
                PathCommand::MoveTo(p) | PathCommand::LineTo(p) => *p = transform.apply(*p),
                PathCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => {
                    *control1 = transform.apply(*control1);
                    *control2 = transform.apply(*control2);
                    *to = transform.apply(*to);
                }
                PathCommand::Close => {}
            }
        }
        stroke
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{PageGeometry, Rect, Rotation};

    #[test]
    fn successive_edits_keep_capture_and_transform_all_bezier_controls_without_remodeling() {
        let page = PageGeometry::new(Rect::new(0., 0., 600., 800.), Rotation::R0, 1.).unwrap();
        let stroke = crate::ink::test_support::pressure_stroke(page).unwrap();
        let move_ink = InkTransform {
            translation: Point::new(25., 31.),
            ..Default::default()
        };
        let rotate = InkTransform {
            center: Point::new(350., 370.),
            radians: 0.71,
            ..Default::default()
        };
        let moved = stroke.transformed(move_ink);
        let edited = moved.transformed(rotate);
        assert_eq!(edited.raw_samples, stroke.raw_samples);
        for sample in &stroke.raw_samples {
            let expected = rotate.apply(move_ink.apply(sample.position));
            let actual = edited.edit_transform.apply(sample.position);
            assert!((actual.x - expected.x).abs() < 1e-9 && (actual.y - expected.y).abs() < 1e-9);
        }
        assert!(
            stroke
                .outline
                .iter()
                .any(|c| matches!(c, PathCommand::CubicTo { .. }))
        );
        for (after, before) in edited.outline.iter().zip(&moved.outline) {
            match (after, before) {
                (PathCommand::MoveTo(a), PathCommand::MoveTo(b))
                | (PathCommand::LineTo(a), PathCommand::LineTo(b)) => {
                    assert_eq!(*a, rotate.apply(*b))
                }
                (
                    PathCommand::CubicTo {
                        control1: a,
                        control2: b,
                        to: c,
                    },
                    PathCommand::CubicTo {
                        control1: x,
                        control2: y,
                        to: z,
                    },
                ) => {
                    assert_eq!(*a, rotate.apply(*x));
                    assert_eq!(*b, rotate.apply(*y));
                    assert_eq!(*c, rotate.apply(*z));
                }
                (PathCommand::Close, PathCommand::Close) => {}
                _ => panic!("edit must preserve command topology"),
            }
        }
    }

    #[test]
    fn old_version_one_records_omit_transform_and_decode_as_identity() {
        let page = PageGeometry::new(Rect::new(0., 0., 600., 800.), Rotation::R0, 1.).unwrap();
        let stroke = crate::ink::test_support::pressure_stroke(page).unwrap();
        let json = serde_json::to_value(&stroke).unwrap();
        assert!(json.get("edit_transform").is_none());
        let mut bytes = Vec::new();
        ciborium::into_writer(&stroke, &mut bytes).unwrap();
        let decoded: StrokeRecord = ciborium::from_reader(bytes.as_slice()).unwrap();
        assert_eq!(decoded, stroke);
        assert!(decoded.edit_transform.is_identity());
    }
}
