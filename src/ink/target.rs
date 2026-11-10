//! Frozen Down-time mapping from physical input to the original page's paper space.

use crate::document::{CanvasViewport, InkTransform, PageGeometry, Point, ViewTransform};

#[derive(Clone, Copy)]
pub struct StrokeTarget {
    pub page_index: u32,
    pub original: PageGeometry,
    pub displayed: PageGeometry,
    pub view: ViewTransform,
    pub viewport: CanvasViewport,
}

impl StrokeTarget {
    pub fn physical_position(self, paper: Point) -> Point {
        let displayed = self
            .displayed
            .pdf_to_paper(self.original.paper_to_pdf(paper));
        self.viewport
            .logical_to_physical(self.view.paper_to_logical(displayed))
    }

    /// Both pages share one frozen viewport and scale. Mapping their original
    /// paper spaces is rigid even with different CropBoxes, rotations and UserUnits.
    pub fn transform_to(self, destination: Self) -> InkTransform {
        let source_rotation =
            self.displayed.rotation().degrees() - self.original.rotation().degrees();
        let destination_rotation =
            destination.displayed.rotation().degrees() - destination.original.rotation().degrees();
        InkTransform {
            translation: destination.paper_position(self.physical_position(Point::default())),
            radians: f64::from(source_rotation - destination_rotation).to_radians(),
            ..Default::default()
        }
    }

    pub fn paper_position(self, physical: Point) -> Point {
        let displayed = self
            .view
            .logical_to_paper(self.viewport.physical_to_logical(physical));
        self.original
            .pdf_to_paper(self.displayed.paper_to_pdf(displayed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Rect, Rotation};

    #[test]
    fn physical_input_maps_to_original_paper_across_rotation_zoom_and_dpi() {
        for rotation in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
            let original =
                PageGeometry::new(Rect::new(-20.0, 30.0, 580.0, 830.0), rotation, 2.5).unwrap();
            let point = Point::new(100.0, 200.0);
            for display in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
                let displayed = original.with_rotation(display);
                for dpi in [1.0, 1.5, 2.0] {
                    for zoom in [0.1, 1.0, 8.0] {
                        let viewport = CanvasViewport::new(
                            Rect::new(0.0, 40.0, 800.0, 640.0),
                            dpi,
                            (1600, 1280),
                        )
                        .unwrap();
                        let mut view = ViewTransform::default();
                        view.set_zoom_at(zoom, Point::new(12.0, 34.0));
                        view.pan(Point::new(-50.0, 70.0));
                        let target = StrokeTarget {
                            page_index: 2,
                            original,
                            displayed,
                            view,
                            viewport,
                        };
                        let display_point = displayed.pdf_to_paper(original.paper_to_pdf(point));
                        let physical =
                            viewport.logical_to_physical(view.paper_to_logical(display_point));
                        let actual = target.paper_position(physical);
                        assert!(
                            (actual.x - point.x).abs() < 1e-8 && (actual.y - point.y).abs() < 1e-8
                        );
                    }
                }
            }
        }
    }
}
