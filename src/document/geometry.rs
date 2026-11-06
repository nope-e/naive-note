use anyhow::{Result, ensure};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    pub fn is_valid(self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width > 0.0 && self.height > 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Rect {
    pub const fn new(left: f64, top: f64, right: f64, bottom: f64) -> Self {
        Self {
            min: Point::new(left, top),
            max: Point::new(right, bottom),
        }
    }

    pub fn size(self) -> Size {
        Size::new(self.max.x - self.min.x, self.max.y - self.min.y)
    }

    pub fn center(self) -> Point {
        Point::new(
            self.min.x + self.size().width * 0.5,
            self.min.y + self.size().height * 0.5,
        )
    }

    pub fn is_valid(self) -> bool {
        self.min.is_finite() && self.max.is_finite() && self.size().is_valid()
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }
}

/// PDF /Rotate: clockwise rotation when displaying the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotation {
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    pub fn next(self) -> Self {
        match self {
            Self::R0 => Self::R90,
            Self::R90 => Self::R180,
            Self::R180 => Self::R270,
            Self::R270 => Self::R0,
        }
    }

    pub fn degrees(self) -> i32 {
        match self {
            Self::R0 => 0,
            Self::R90 => 90,
            Self::R180 => 180,
            Self::R270 => 270,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageGeometry {
    // PDF coordinates: bottom-left origin, positive y points up. Rect.min/max
    // are numerical bounds here, not the visual top/bottom of the page.
    crop_box: Rect,
    rotation: Rotation,
    user_unit: f64,
}

impl PageGeometry {
    pub fn new(crop_box: Rect, rotation: Rotation, user_unit: f64) -> Result<Self> {
        ensure!(crop_box.is_valid(), "invalid PDF CropBox");
        ensure!(
            user_unit.is_finite() && user_unit > 0.0,
            "invalid PDF UserUnit"
        );
        let page = Self {
            crop_box,
            rotation,
            user_unit,
        };
        ensure!(
            page.unrotated_size().is_valid(),
            "PDF page dimensions overflow"
        );
        Ok(page)
    }

    pub fn crop_box(self) -> Rect {
        self.crop_box
    }

    pub fn rotation(self) -> Rotation {
        self.rotation
    }

    pub fn user_unit(self) -> f64 {
        self.user_unit
    }

    pub fn with_rotation(self, rotation: Rotation) -> Self {
        Self { rotation, ..self }
    }

    /// Physical paper points, 1/72 inch, before display rotation.
    pub fn unrotated_size(self) -> Size {
        let size = self.crop_box.size();
        Size::new(size.width * self.user_unit, size.height * self.user_unit)
    }

    /// Rotated paper bounds with a top-left origin and positive y pointing down.
    pub fn paper_size(self) -> Size {
        let size = self.unrotated_size();
        match self.rotation {
            Rotation::R0 | Rotation::R180 => size,
            Rotation::R90 | Rotation::R270 => Size::new(size.height, size.width),
        }
    }

    pub fn pdf_to_paper(self, pdf: Point) -> Point {
        let u = (pdf.x - self.crop_box.min.x) * self.user_unit;
        let v = (pdf.y - self.crop_box.min.y) * self.user_unit;
        let Size { width, height } = self.unrotated_size();
        match self.rotation {
            Rotation::R0 => Point::new(u, height - v),
            Rotation::R90 => Point::new(v, u),
            Rotation::R180 => Point::new(width - u, v),
            Rotation::R270 => Point::new(height - v, width - u),
        }
    }

    pub fn paper_to_pdf(self, paper: Point) -> Point {
        let Size { width, height } = self.unrotated_size();
        let (u, v) = match self.rotation {
            Rotation::R0 => (paper.x, height - paper.y),
            Rotation::R90 => (paper.y, paper.x),
            Rotation::R180 => (width - paper.x, paper.y),
            Rotation::R270 => (width - paper.y, height - paper.x),
        };
        Point::new(
            u / self.user_unit + self.crop_box.min.x,
            v / self.user_unit + self.crop_box.min.y,
        )
    }

    pub fn contains_paper(self, paper: Point) -> bool {
        let size = self.paper_size();
        Rect::new(0.0, 0.0, size.width, size.height).contains(paper)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(actual: Point, expected: Point) {
        assert!(
            (actual.x - expected.x).abs() < 1e-9,
            "{actual:?} != {expected:?}"
        );
        assert!(
            (actual.y - expected.y).abs() < 1e-9,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn crop_offset_user_unit_and_rotation_have_expected_coordinates() {
        // Asymmetric interior point distinguishes all four rotations.
        for (rotation, expected, size) in [
            (
                Rotation::R0,
                Point::new(40.0, 260.0),
                Size::new(200.0, 300.0),
            ),
            (
                Rotation::R90,
                Point::new(40.0, 40.0),
                Size::new(300.0, 200.0),
            ),
            (
                Rotation::R180,
                Point::new(160.0, 40.0),
                Size::new(200.0, 300.0),
            ),
            (
                Rotation::R270,
                Point::new(260.0, 160.0),
                Size::new(300.0, 200.0),
            ),
        ] {
            let page =
                PageGeometry::new(Rect::new(-10.0, 25.0, 90.0, 175.0), rotation, 2.0).unwrap();
            near(page.pdf_to_paper(Point::new(10.0, 45.0)), expected);
            near(page.paper_to_pdf(expected), Point::new(10.0, 45.0));
            assert_eq!(page.paper_size(), size);
        }
    }

    #[test]
    fn corners_and_fractional_points_round_trip_for_all_rotations() {
        let crop = Rect::new(18.25, -40.5, 494.25, 633.1);
        for rotation in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
            let page = PageGeometry::new(crop, rotation, 1.25).unwrap();
            for pdf in [
                crop.min,
                crop.max,
                Point::new(crop.min.x, crop.max.y),
                Point::new(crop.max.x, crop.min.y),
                Point::new(72.75, 321.125),
            ] {
                let paper = page.pdf_to_paper(pdf);
                assert!(page.contains_paper(paper));
                near(page.paper_to_pdf(paper), pdf);
            }
            let size = page.paper_size();
            assert!(!page.contains_paper(Point::new(-0.01, 0.0)));
            assert!(!page.contains_paper(Point::new(size.width, size.height + 0.01)));
        }
    }

    #[test]
    fn rejects_invalid_page_geometry() {
        for crop in [
            Rect::new(0.0, 0.0, 0.0, 10.0),
            Rect::new(10.0, 0.0, 0.0, 10.0),
            Rect::new(f64::NAN, 0.0, 10.0, 10.0),
            Rect::new(0.0, 0.0, f64::INFINITY, 10.0),
        ] {
            assert!(PageGeometry::new(crop, Rotation::R0, 1.0).is_err());
        }
        for unit in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX] {
            assert!(
                PageGeometry::new(Rect::new(0.0, 0.0, 595.0, 842.0), Rotation::R0, unit).is_err()
            );
        }
    }
}
