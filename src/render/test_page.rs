//! Temporary paper content. All positions are paper points, not framebuffer pixels.

use skia_safe::{Canvas, Color, Paint, Rect as SkRect, paint::Style};

use crate::document::{PageGeometry, Point};

pub(super) fn draw_contents(canvas: &Canvas, page: PageGeometry) {
    let size = page.unrotated_size();
    let (width, height) = (size.width, size.height);
    let crop = page.crop_box();
    // Construct original-orientation paper content in PDF user space, then
    // project through the same CropBox/UserUnit/rotation mapping as hit-testing.
    let project = |x: f64, y: f64| {
        let point = page.pdf_to_paper(Point::new(
            crop.min.x + x / page.user_unit(),
            crop.max.y - y / page.user_unit(),
        ));
        (point.x as f32, point.y as f32)
    };
    let mut paint = Paint::default();
    paint.set_anti_alias(true).set_style(Style::Stroke);

    // 18 pt grid; stronger 72 pt (one inch) lines. Endpoints deliberately
    // extend past the CropBox so the page clip can be inspected manually.
    for index in 1..=(width / 18.0) as u32 {
        let x = f64::from(index) * 18.0;
        grid_paint(&mut paint, index);
        canvas.draw_line(project(x, -24.0), project(x, height + 24.0), &paint);
    }
    for index in 1..=(height / 18.0) as u32 {
        let y = f64::from(index) * 18.0;
        grid_paint(&mut paint, index);
        canvas.draw_line(project(-24.0, y), project(width + 24.0, y), &paint);
    }

    paint
        .set_stroke_width(1.0)
        .set_color(Color::from_rgb(0xB8, 0xCE, 0xC8));
    for (a, b) in [
        ((36.0, 36.0), (width - 36.0, 36.0)),
        ((width - 36.0, 36.0), (width - 36.0, height - 36.0)),
        ((width - 36.0, height - 36.0), (36.0, height - 36.0)),
        ((36.0, height - 36.0), (36.0, 36.0)),
    ] {
        canvas.draw_line(project(a.0, a.1), project(b.0, b.1), &paint);
    }

    let center = project(width * 0.5, height * 0.5);
    paint
        .set_style(Style::Fill)
        .set_color(Color::from_rgb(0x14, 0xB8, 0xA6));
    canvas.draw_circle(center, 54.0, &paint);
    paint
        .set_style(Style::Stroke)
        .set_stroke_width(2.0)
        .set_color(Color::from_rgb(0x0F, 0x76, 0x6E));
    canvas.draw_circle(center, 54.0, &paint);

    // Distinct corner markers make clockwise rotation easy to identify.
    paint
        .set_style(Style::Fill)
        .set_color(Color::from_rgb(0xDF, 0x54, 0x52));
    canvas.draw_circle(project(36.0, 36.0), 9.0, &paint);
    paint
        .set_style(Style::Stroke)
        .set_stroke_width(3.0)
        .set_color(Color::from_rgb(0x3B, 0x98, 0x59));
    let a = project(width - 46.0, 26.0);
    let b = project(width - 26.0, 46.0);
    canvas.draw_rect(
        SkRect::new(a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)),
        &paint,
    );
    paint.set_color(Color::from_rgb(0x4A, 0x89, 0xCD));
    canvas.draw_line(
        project(26.0, height - 36.0),
        project(46.0, height - 36.0),
        &paint,
    );
    canvas.draw_line(
        project(36.0, height - 46.0),
        project(36.0, height - 26.0),
        &paint,
    );
    paint.set_color(Color::from_rgb(0xE0, 0x92, 0x32));
    let (x, y) = (width - 36.0, height - 36.0);
    for (a, b) in [
        ((x, y - 11.0), (x + 11.0, y)),
        ((x + 11.0, y), (x, y + 11.0)),
        ((x, y + 11.0), (x - 11.0, y)),
        ((x - 11.0, y), (x, y - 11.0)),
    ] {
        canvas.draw_line(project(a.0, a.1), project(b.0, b.1), &paint);
    }
}

fn grid_paint(paint: &mut Paint, index: u32) {
    if index.is_multiple_of(4) {
        paint
            .set_stroke_width(0.7)
            .set_color(Color::from_rgb(0xCF, 0xDD, 0xD9));
    } else {
        paint
            .set_stroke_width(0.35)
            .set_color(Color::from_rgb(0xE6, 0xED, 0xEA));
    }
}
