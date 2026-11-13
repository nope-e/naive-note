//! Bound only the interoperable PDF appearance; editable geometry stays exact.

use std::borrow::Cow;

use crate::document::{InkKind, ModeledSample, PathCommand, PenStyle, Point, StrokeRecord};

// Paper points, independent of zoom, CropBox, page rotation and UserUnit.
const TOLERANCE_PT: f64 = 0.03;

pub(super) struct PdfGeometry<'a> {
    pub samples: Cow<'a, [ModeledSample]>,
    pub outline: Cow<'a, [PathCommand]>,
}

pub(super) fn for_pdf(stroke: &StrokeRecord, version: u32) -> PdfGeometry<'_> {
    let original = || PdfGeometry {
        samples: Cow::Borrowed(stroke.modeled_samples.as_slice()),
        outline: Cow::Borrowed(stroke.outline.as_slice()),
    };
    // Custom/legacy outlines remain authoritative. Highlighters already have a
    // separate contour coalescer; do not compound its approximation here.
    if version == 1 || stroke.style.kind != InkKind::Pen || stroke.modeled_samples.len() < 3 {
        return original();
    }
    let replay = crate::ink::saved_outline_v1(&stroke.modeled_samples, &stroke.style);
    if !equivalent(&replay, &stroke.outline) {
        return original();
    }
    let samples = simplify(&stroke.modeled_samples, &stroke.style);
    if samples.len() == stroke.modeled_samples.len() {
        return original();
    }
    PdfGeometry {
        outline: Cow::Owned(crate::ink::saved_outline_v1(&samples, &stroke.style)),
        samples: Cow::Owned(samples),
    }
}

fn equivalent(a: &[PathCommand], b: &[PathCommand]) -> bool {
    // Rigid edits may differ from replay by floating-point roundoff. This is
    // far smaller than the PDF approximation budget and never rewrites metadata.
    let point = |a: Point, b: Point| (a.x - b.x).abs().max((a.y - b.y).abs()) <= 1e-7;
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| match (a, b) {
            (PathCommand::MoveTo(a), PathCommand::MoveTo(b))
            | (PathCommand::LineTo(a), PathCommand::LineTo(b)) => point(*a, *b),
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
            ) => point(*a, *x) && point(*b, *y) && point(*c, *z),
            (PathCommand::Close, PathCommand::Close) => true,
            _ => false,
        })
}

fn simplify(samples: &[ModeledSample], style: &PenStyle) -> Vec<ModeledSample> {
    let radii: Vec<_> = samples
        .iter()
        .map(|s| style.width(s.pressure) * 0.5)
        .collect();
    let mut keep = vec![false; samples.len()];
    // Bound RDP's worst case on jagged input and avoid recursive stack growth.
    const SPAN: usize = 256;
    for start in (0..samples.len() - 1).step_by(SPAN) {
        let end = (start + SPAN).min(samples.len() - 1);
        keep[start] = true;
        keep[end] = true;
        let mut pending = vec![(start, end)];
        while let Some((a, b)) = pending.pop() {
            let mut worst = TOLERANCE_PT;
            let mut split = None;
            for i in a + 1..b {
                let error = error(samples, &radii, a, b, i);
                if error > worst {
                    worst = error;
                    split = Some(i);
                }
            }
            if let Some(i) = split {
                keep[i] = true;
                pending.extend([(a, i), (i, b)]);
            }
        }
    }
    samples
        .iter()
        .zip(keep)
        .filter(|(_, keep)| *keep)
        .map(|(s, _)| s.clone())
        .collect()
}

fn error(samples: &[ModeledSample], radii: &[f64], a: usize, b: usize, i: usize) -> f64 {
    let from = samples[a].position;
    let to = samples[b].position;
    let p = samples[i].position;
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let length2 = dx * dx + dy * dy;
    let t = if length2 > f64::EPSILON {
        (((p.x - from.x) * dx + (p.y - from.y) * dy) / length2).clamp(0., 1.)
    } else {
        // Stationary pressure changes are still significant geometry.
        (i - a) as f64 / (b - a) as f64
    };
    (p.x - (from.x + t * dx)).hypot(p.y - (from.y + t * dy))
        + (radii[i] - (radii[a] + t * (radii[b] - radii[a]))).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dense_curve_reduces_samples_with_bounded_position_and_radius_error() {
        let style = PenStyle::default();
        let samples: Vec<_> = (0..1000)
            .map(|i| {
                let t = i as f64 / 999.;
                ModeledSample {
                    position: Point::new(t * 100., (t * 8.).sin() * 10.),
                    pressure: 0.5 + (t * 10.).sin() * 0.4,
                    time_us: i,
                }
            })
            .collect();
        let simplified = simplify(&samples, &style);
        assert!(simplified.len() < samples.len() / 5);
        assert_eq!(simplified.first(), samples.first());
        assert_eq!(simplified.last(), samples.last());
        let radii: Vec<_> = samples
            .iter()
            .map(|s| style.width(s.pressure) * 0.5)
            .collect();
        for segment in simplified.windows(2) {
            let a = segment[0].time_us as usize;
            let b = segment[1].time_us as usize;
            for i in a + 1..b {
                assert!(error(&samples, &radii, a, b, i) <= TOLERANCE_PT);
            }
        }
    }

    #[test]
    fn stationary_pressure_spikes_corners_and_reversals_survive() {
        let style = PenStyle::default();
        for positions in [[0., 0., 0.], [0., 10., 0.], [0., 10., 20.]] {
            let samples: Vec<_> = positions
                .into_iter()
                .enumerate()
                .map(|(i, x)| ModeledSample {
                    position: Point::new(x, 0.),
                    time_us: i as u64,
                    pressure: if i == 1 { 1. } else { 0.1 },
                })
                .collect();
            assert_eq!(simplify(&samples, &style), samples);
        }
    }

    #[test]
    fn custom_outlines_and_highlighters_are_retained() {
        let page = crate::document::PageGeometry::new(
            crate::document::Rect::new(0., 0., 600., 800.),
            crate::document::Rotation::R0,
            1.,
        )
        .unwrap();
        let mut stroke = crate::ink::test_support::pressure_stroke(page).unwrap();
        stroke.outline[0] = PathCommand::MoveTo(Point::new(-10., -20.));
        assert!(matches!(for_pdf(&stroke, 2).outline, Cow::Borrowed(_)));
        stroke.style.kind = InkKind::Highlighter;
        assert!(matches!(for_pdf(&stroke, 2).outline, Cow::Borrowed(_)));
    }
}
