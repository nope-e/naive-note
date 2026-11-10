use crate::document::{ModeledSample, PathCommand, PenStyle, Point};

/// Highlighters are wide enough that a stationary modeler can otherwise put
/// hundreds of coincident contours into one fill. Preserve every modeled/raw
/// sample, but coalesce its redundant geometry around the last emitted disk.
#[derive(Default)]
pub(super) struct OutlineState {
    anchor: Option<ModeledSample>,
    disk_start: usize,
}

impl OutlineState {
    pub fn anchor(&self) -> Option<&ModeledSample> {
        self.anchor.as_ref()
    }

    pub fn append(
        &mut self,
        path: &mut Vec<PathCommand>,
        sample: &ModeledSample,
        style: &PenStyle,
        endpoint: bool,
    ) {
        if style.kind == crate::document::InkKind::Highlighter
            && let Some(anchor) = &self.anchor
        {
            let distance = (sample.position.x - anchor.position.x)
                .hypot(sample.position.y - anchor.position.y);
            let old_radius = style.width(anchor.pressure) * 0.5;
            let radius = style.width(sample.pressure) * 0.5;
            if distance + radius <= old_radius + 1e-9 {
                return;
            }
            if distance + old_radius <= radius + 1e-9 {
                // The new disk covers the preceding disk, including its joins.
                // Replace it rather than accumulating concentric contours.
                path.truncate(self.disk_start);
                append_outline(path, None, sample, style);
                self.anchor = Some(sample.clone());
                return;
            }
            // Bound overlapping contours from subpixel modeler settling. The
            // next emitted segment still joins the last emitted anchor, and Up
            // always includes the actual endpoint. Width changes stay visible.
            let tolerance = old_radius.max(radius) * 0.05;
            if !endpoint && distance < tolerance && (radius - old_radius).abs() < tolerance {
                return;
            }
        }
        append_outline(path, self.anchor.as_ref(), sample, style);
        self.disk_start = path.len() - 6;
        self.anchor = Some(sample.clone());
    }
}

// Round disks and connecting trapezoids use the same winding. Nonzero filling
// combines them without seams and handles coincident samples without division.
#[cfg(test)]
pub(super) fn round_outline(samples: &[ModeledSample], style: &PenStyle) -> Vec<PathCommand> {
    let mut path = Vec::new();
    let mut state = OutlineState::default();
    for (index, sample) in samples.iter().enumerate() {
        state.append(&mut path, sample, style, index + 1 == samples.len());
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
