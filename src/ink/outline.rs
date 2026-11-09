use crate::document::{ModeledSample, PathCommand, PenStyle, Point};

// Round disks and connecting trapezoids use the same winding. Nonzero filling
// combines them without seams and handles coincident samples without division.
pub(super) fn round_outline(samples: &[ModeledSample], style: &PenStyle) -> Vec<PathCommand> {
    let mut path = Vec::new();
    for (i, sample) in samples.iter().enumerate() {
        append_outline(
            &mut path,
            i.checked_sub(1).map(|i| &samples[i]),
            sample,
            style,
        );
    }
    path
}

pub(super) fn append_outline(
    path: &mut Vec<PathCommand>,
    previous: Option<&ModeledSample>,
    sample: &ModeledSample,
    style: &PenStyle,
) {
    if let Some(a) = previous {
        let b = sample;
        let dx = b.position.x - a.position.x;
        let dy = b.position.y - a.position.y;
        let length = dx.hypot(dy);
        if length > f64::EPSILON {
            let offset = |sample: &ModeledSample, sign: f64| {
                let radius = style.width(sample.pressure) * 0.5;
                Point::new(
                    sample.position.x - sign * dy / length * radius,
                    sample.position.y + sign * dx / length * radius,
                )
            };
            path.extend([
                PathCommand::MoveTo(offset(a, -1.0)),
                PathCommand::LineTo(offset(b, -1.0)),
                PathCommand::LineTo(offset(b, 1.0)),
                PathCommand::LineTo(offset(a, 1.0)),
                PathCommand::Close,
            ]);
        }
    }
    {
        let p = sample.position;
        let r = style.width(sample.pressure) * 0.5;
        let k = r * 0.552_284_749_830_793_6;
        path.extend([
            PathCommand::MoveTo(Point::new(p.x + r, p.y)),
            PathCommand::CubicTo {
                control1: Point::new(p.x + r, p.y + k),
                control2: Point::new(p.x + k, p.y + r),
                to: Point::new(p.x, p.y + r),
            },
            PathCommand::CubicTo {
                control1: Point::new(p.x - k, p.y + r),
                control2: Point::new(p.x - r, p.y + k),
                to: Point::new(p.x - r, p.y),
            },
            PathCommand::CubicTo {
                control1: Point::new(p.x - r, p.y - k),
                control2: Point::new(p.x - k, p.y - r),
                to: Point::new(p.x, p.y - r),
            },
            PathCommand::CubicTo {
                control1: Point::new(p.x + k, p.y - r),
                control2: Point::new(p.x + r, p.y - k),
                to: Point::new(p.x + r, p.y),
            },
            PathCommand::Close,
        ]);
    }
}
