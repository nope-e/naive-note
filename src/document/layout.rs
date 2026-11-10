//! Continuous vertical paper layout. All positions are in physical paper points.

use super::{PageGeometry, Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutPage {
    pub index: u32,
    pub geometry: PageGeometry,
    pub bounds: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageLayout {
    pages: Vec<LayoutPage>,
    size: Size,
}

impl PageLayout {
    pub fn new(geometries: &[PageGeometry]) -> Self {
        assert!(!geometries.is_empty());
        let width = geometries
            .iter()
            .map(|page| page.paper_size().width)
            .fold(0.0, f64::max);
        let mut top = 0.0;
        let pages = geometries
            .iter()
            .enumerate()
            .map(|(index, &geometry)| {
                let size = geometry.paper_size();
                let left = (width - size.width) * 0.5;
                let bounds = Rect::new(left, top, left + size.width, top + size.height);
                top += size.height;
                LayoutPage {
                    index: index as u32,
                    geometry,
                    bounds,
                }
            })
            .collect();
        Self {
            pages,
            size: Size::new(width, top),
        }
    }

    pub fn pages(&self) -> &[LayoutPage] {
        &self.pages
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn visible(&self, area: Rect) -> impl Iterator<Item = &LayoutPage> {
        let first = self
            .pages
            .partition_point(|page| page.bounds.max.y <= area.min.y);
        self.pages[first..]
            .iter()
            .take_while(move |page| page.bounds.min.y < area.max.y)
            .filter(move |page| page.bounds.min.x < area.max.x && page.bounds.max.x > area.min.x)
    }

    pub fn nearest(&self, point: Point) -> usize {
        // Adjacent pages share an edge; assign that edge to the following page.
        self.pages
            .partition_point(|page| page.bounds.max.y <= point.y)
            .min(self.pages.len() - 1)
    }

    pub fn hit_test(&self, point: Point) -> Option<&LayoutPage> {
        let page = &self.pages[self.nearest(point)];
        page.bounds.contains(point).then_some(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Rotation;

    fn layout() -> PageLayout {
        PageLayout::new(&[
            PageGeometry::new(Rect::new(20.0, 30.0, 220.0, 330.0), Rotation::R0, 1.5).unwrap(),
            PageGeometry::new(Rect::new(-10.0, 40.0, 390.0, 240.0), Rotation::R90, 2.0).unwrap(),
        ])
    }

    #[test]
    fn mixed_pages_are_centered_connected_and_culled() {
        let layout = layout();
        assert_eq!(layout.pages[0].bounds, Rect::new(50.0, 0.0, 350.0, 450.0));
        assert_eq!(layout.pages[1].bounds, Rect::new(0.0, 450.0, 400.0, 1250.0));
        assert_eq!(layout.size(), Size::new(400.0, 1250.0));
        assert_eq!(
            layout.visible(Rect::new(0.0, 440.0, 400.0, 490.0)).count(),
            2
        );
        assert_eq!(
            layout.visible(Rect::new(0.0, 450.0, 400.0, 474.0)).count(),
            1
        );
        assert_eq!(
            layout.visible(Rect::new(401.0, 0.0, 500.0, 1400.0)).count(),
            0
        );
    }

    #[test]
    fn shared_edges_hit_the_following_page_and_margins_do_not_hit() {
        let layout = layout();
        assert_eq!(layout.hit_test(Point::new(200.0, 449.9)).unwrap().index, 0);
        assert_eq!(layout.hit_test(Point::new(10.0, 450.0)).unwrap().index, 1);
        assert_eq!(layout.hit_test(Point::new(200.0, 460.0)).unwrap().index, 1);
        assert!(layout.hit_test(Point::new(10.0, 100.0)).is_none());
        assert_eq!(layout.nearest(Point::new(0.0, -1000.0)), 0);
        assert_eq!(layout.nearest(Point::new(0.0, 10000.0)), 1);
        let page = layout.hit_test(Point::new(120.0, 600.0)).unwrap();
        let local = Point::new(120.0 - page.bounds.min.x, 600.0 - page.bounds.min.y);
        let pdf = page.geometry.paper_to_pdf(local);
        assert_eq!(page.geometry.pdf_to_paper(pdf), local);
        assert_eq!(page.index, 1);
    }
}
