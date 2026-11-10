//! Reference overlay, after PDF content and before session ink.

use crate::document::{PageGeometry, Point, Rect, ReferenceLines, ReferenceMark};
use skia_safe::{Canvas, Color, Paint, Rect as SkRect, paint::Style};

pub(super) fn draw_contents(canvas: &Canvas, page: PageGeometry, lines: ReferenceLines) {
    let size = page.unrotated_size();
    let crop = page.crop_box();
    let project = |p: Point| {
        let p = page.pdf_to_paper(Point::new(
            crop.min.x + p.x / page.user_unit(),
            crop.max.y - p.y / page.user_unit(),
        ));
        (p.x as f32, p.y as f32)
    };
    let [r, g, b] = lines.color_rgb;
    let mut paint = Paint::default();
    paint
        .set_anti_alias(true)
        .set_style(Style::Stroke)
        .set_stroke_width(0.45)
        .set_color(Color::from_rgb(r, g, b));
    let Some(clip) = canvas.local_clip_bounds() else {
        return;
    };
    let corners = [
        Point::new(f64::from(clip.left), f64::from(clip.top)),
        Point::new(f64::from(clip.right), f64::from(clip.bottom)),
    ];
    let corners = corners.map(|p| {
        let pdf = page.paper_to_pdf(p);
        Point::new(
            (pdf.x - crop.min.x) * page.user_unit(),
            (crop.max.y - pdf.y) * page.user_unit(),
        )
    });
    let bounds = Rect::new(
        corners[0].x.min(corners[1].x).max(0.0),
        corners[0].y.min(corners[1].y).max(0.0),
        corners[0].x.max(corners[1].x).min(size.width),
        corners[0].y.max(corners[1].y).min(size.height),
    );
    let _ = lines.visit(bounds, |mark| match mark {
        ReferenceMark::Line(a, b) => {
            paint.set_style(Style::Stroke);
            canvas.draw_line(project(a), project(b), &paint);
        }
        ReferenceMark::Dot(p) => {
            paint.set_style(Style::Fill);
            let (x, y) = project(p);
            canvas.draw_rect(SkRect::from_xywh(x - 0.7, y - 0.7, 1.4, 1.4), &paint);
        }
    });
}
