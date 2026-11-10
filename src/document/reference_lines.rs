//! Page-local reference geometry shared by the canvas and PDF serialization.

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use super::{Point, Rect};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferencePattern {
    None,
    Lines,
    #[default]
    Grid,
    Dots,
    IsometricGrid,
    IsometricDots,
}

impl ReferencePattern {
    pub const ALL: [(Self, &'static str); 6] = [
        (Self::None, "无"),
        (Self::Lines, "线"),
        (Self::Grid, "方格"),
        (Self::Dots, "点"),
        (Self::IsometricGrid, "等距网格"),
        (Self::IsometricDots, "等距点阵"),
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReferenceLines {
    pub pattern: ReferencePattern,
    pub color_rgb: [u8; 3],
    /// Physical PDF points, independent of CropBox, rotation and UserUnit.
    pub width: f64,
    pub height: f64,
}

impl Default for ReferenceLines {
    fn default() -> Self {
        Self {
            pattern: ReferencePattern::Grid,
            color_rgb: [0xDC, 0xEB, 0xFD],
            width: 18.0,
            height: 18.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReferenceLayer {
    pub lines: ReferenceLines,
    /// Include ordinary vector page content for other PDF readers and printing.
    pub embed_in_pdf: bool,
    /// Show the last-page guide and append paper while writing past it.
    /// This aid is stored in edit metadata only, never in PDF page content.
    #[serde(default = "default_auto_expand")]
    pub auto_expand: bool,
}

fn default_auto_expand() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReferenceMark {
    Line(Point, Point),
    Dot(Point),
}

impl ReferenceLines {
    pub fn validate(self) -> Result<()> {
        ensure!(
            self.width.is_finite() && (4.0..=256.0).contains(&self.width),
            "无效的参考线纹理宽度"
        );
        ensure!(
            self.height.is_finite() && (4.0..=256.0).contains(&self.height),
            "无效的参考线纹理高度"
        );
        Ok(())
    }

    /// Generate only geometry intersecting the requested page-local region.
    pub fn visit(self, bounds: Rect, mut emit: impl FnMut(ReferenceMark)) -> Result<()> {
        self.validate()?;
        if !bounds.is_valid() || self.pattern == ReferencePattern::None {
            return Ok(());
        }
        let x0 = (bounds.min.x / self.width).floor().max(0.0) as u32;
        let x1 = (bounds.max.x / self.width).ceil().max(0.0) as u32;
        let y0 = (bounds.min.y / self.height).floor().max(0.0) as u32;
        let y1 = (bounds.max.y / self.height).ceil().max(0.0) as u32;
        ensure!(
            u64::from(x1 - x0) + u64::from(y1 - y0) <= 50_000,
            "页面参考线过密，请增大纹理间距"
        );
        match self.pattern {
            ReferencePattern::Lines | ReferencePattern::Grid => {
                for row in y0.max(1)..=y1 {
                    let y = f64::from(row) * self.height;
                    if y <= bounds.max.y {
                        emit(ReferenceMark::Line(
                            Point::new(bounds.min.x, y),
                            Point::new(bounds.max.x, y),
                        ));
                    }
                }
                if self.pattern == ReferencePattern::Grid {
                    for col in x0.max(1)..=x1 {
                        let x = f64::from(col) * self.width;
                        if x <= bounds.max.x {
                            emit(ReferenceMark::Line(
                                Point::new(x, bounds.min.y),
                                Point::new(x, bounds.max.y),
                            ));
                        }
                    }
                }
            }
            ReferencePattern::Dots | ReferencePattern::IsometricDots => {
                ensure!(
                    (u64::from(x1 - x0) + 1) * (u64::from(y1 - y0) + 1) <= 100_000,
                    "页面参考点过密，请增大纹理间距"
                );
                for row in y0.max(1)..=y1 {
                    let shift = if self.pattern == ReferencePattern::IsometricDots && row % 2 == 1 {
                        self.width / 2.0
                    } else {
                        0.0
                    };
                    for col in x0..=x1 {
                        let point = Point::new(
                            f64::from(col) * self.width + shift,
                            f64::from(row) * self.height,
                        );
                        if point.x > 0.0 && bounds.contains(point) {
                            emit(ReferenceMark::Dot(point));
                        }
                    }
                }
            }
            ReferencePattern::IsometricGrid => {
                for row in y0.max(1)..=y1 {
                    let y = f64::from(row) * self.height;
                    emit(ReferenceMark::Line(
                        Point::new(bounds.min.x, y),
                        Point::new(bounds.max.x, y),
                    ));
                }
                let slope = self.width / (2.0 * self.height);
                for sign in [-1.0, 1.0] {
                    let first =
                        ((bounds.min.x - sign * slope * bounds.max.y) / self.width).floor() as i64;
                    let last =
                        ((bounds.max.x - sign * slope * bounds.min.y) / self.width).ceil() as i64;
                    let (first, last) = if sign > 0.0 {
                        (first, last)
                    } else {
                        (
                            ((bounds.min.x - sign * slope * bounds.min.y) / self.width).floor()
                                as i64,
                            ((bounds.max.x - sign * slope * bounds.max.y) / self.width).ceil()
                                as i64,
                        )
                    };
                    for col in first..=last {
                        let x = col as f64 * self.width;
                        emit(ReferenceMark::Line(
                            Point::new(x + sign * slope * bounds.min.y, bounds.min.y),
                            Point::new(x + sign * slope * bounds.max.y, bounds.max.y),
                        ));
                    }
                }
            }
            ReferencePattern::None => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_grid_and_staggered_dots_keep_the_page_origin() {
        let bounds = Rect::new(20.0, 20.0, 65.0, 65.0);
        let mut marks = Vec::new();
        ReferenceLines::default()
            .visit(bounds, |mark| marks.push(mark))
            .unwrap();
        assert!(marks.contains(&ReferenceMark::Line(
            Point::new(36.0, 20.0),
            Point::new(36.0, 65.0)
        )));
        let mut dots = Vec::new();
        ReferenceLines {
            pattern: ReferencePattern::IsometricDots,
            ..ReferenceLines::default()
        }
        .visit(Rect::new(0.0, 0.0, 54.0, 36.0), |mark| dots.push(mark))
        .unwrap();
        assert!(dots.contains(&ReferenceMark::Dot(Point::new(9.0, 18.0))));
        assert!(dots.contains(&ReferenceMark::Dot(Point::new(18.0, 36.0))));
    }

    #[test]
    fn malformed_spacing_and_excessive_full_page_density_are_rejected() {
        for width in [f64::NAN, f64::INFINITY, 0.0, 3.0, 257.0] {
            assert!(
                ReferenceLines {
                    width,
                    ..ReferenceLines::default()
                }
                .validate()
                .is_err()
            );
        }
        let lines = ReferenceLines {
            pattern: ReferencePattern::Dots,
            ..ReferenceLines::default()
        };
        assert!(
            lines
                .visit(Rect::new(0.0, 0.0, 100_000.0, 100_000.0), |_| {})
                .is_err()
        );
        // A small visible region near the bottom of a tall page stays bounded.
        assert!(
            lines
                .visit(Rect::new(0.0, 90_000.0, 100.0, 90_100.0), |_| {})
                .is_ok()
        );
    }
}
