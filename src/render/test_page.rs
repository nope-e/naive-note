//! Light blue squared paper for a new, unsaved document.

use skia_safe::{Canvas, Color, Paint, paint::Style};

use crate::document::{PageGeometry, Point};

pub(super) fn draw_contents(canvas: &Canvas, page: PageGeometry) {
    let size = page.unrotated_size();
    let crop = page.crop_box();
    let project = |x: f64, y: f64| {
        let point = page.pdf_to_paper(Point::new(
            crop.min.x + x / page.user_unit(),
            crop.max.y - y / page.user_unit(),
        ));
        (point.x as f32, point.y as f32)
    };
    let mut paint = Paint::default();
    paint
        .set_anti_alias(true)
        .set_style(Style::Stroke)
        .set_stroke_width(0.45)
        .set_color(Color::from_rgb(0xDC, 0xEB, 0xFD));
    for index in 1..=(size.width / 18.0) as u32 {
        let x = f64::from(index) * 18.0;
        canvas.draw_line(project(x, 0.0), project(x, size.height), &paint);
    }
    for index in 1..=(size.height / 18.0) as u32 {
        let y = f64::from(index) * 18.0;
        canvas.draw_line(project(0.0, y), project(size.width, y), &paint);
    }
}
