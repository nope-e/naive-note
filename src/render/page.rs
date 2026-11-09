//! Paper frame and PDF preview composition; no PDF objects or parsing on the GL thread.

use skia_safe::{
    Canvas, Color, FilterMode, Image, Paint, Rect as SkRect, SamplingOptions, paint::Style,
};

use super::{PageScene, test_page};
use crate::document::{CanvasViewport, PathCommand, PenStyle, Point, StrokeRecord};

pub(super) fn draw(
    canvas: &Canvas,
    scene: &PageScene,
    viewport: CanvasViewport,
    image: Option<&Image>,
    tile_images: &[Image],
) {
    let top_left = viewport.logical_to_physical(viewport.logical.min);
    let bottom_right = viewport.logical_to_physical(viewport.logical.max);
    let size = scene.page.paper_size();
    let paper = SkRect::from_wh(size.width as f32, size.height as f32);
    let dpi = viewport.pixels_per_point as f32;
    let scale = scene.view.scale() as f32;
    let origin = scene.view.paper_to_logical(Point::default());

    canvas.save();
    canvas.clip_rect(
        SkRect::new(
            top_left.x as f32,
            top_left.y as f32,
            bottom_right.x as f32,
            bottom_right.y as f32,
        ),
        None,
        false,
    );
    canvas.scale((dpi, dpi));
    canvas.translate((origin.x as f32, origin.y as f32));
    canvas.scale((scale, scale));

    let mut paint = Paint::default();
    paint
        .set_anti_alias(true)
        .set_color(Color::from_argb(35, 0, 0, 0));
    let shadow_offset = 5.0 / scale;
    canvas.draw_rect(
        SkRect::from_xywh(shadow_offset, shadow_offset, paper.width(), paper.height()),
        &paint,
    );
    paint.set_color(Color::WHITE);
    canvas.draw_rect(paper, &paint);

    canvas.save();
    canvas.clip_rect(paper, None, false);
    // Session ink uses the original page orientation even before its bitmap
    // arrives. Rotate all page content together, including the active stroke.
    let original = scene.source_page.paper_size();
    let degrees =
        (scene.page.rotation().degrees() - scene.source_page.rotation().degrees()).rem_euclid(360);
    match degrees {
        90 => {
            canvas.translate((original.height as f32, 0.0));
        }
        180 => {
            canvas.translate((original.width as f32, original.height as f32));
        }
        270 => {
            canvas.translate((0.0, original.width as f32));
        }
        _ => {}
    }
    canvas.rotate(degrees as f32, None);
    if let (Some(_), Some(image)) = (&scene.bitmap, image) {
        canvas.draw_image_rect_with_sampling_options(
            image,
            None,
            SkRect::from_wh(original.width as f32, original.height as f32),
            SamplingOptions::from(FilterMode::Linear),
            &Paint::default(),
        );
        for (tile, image) in scene.tiles.iter().zip(tile_images) {
            let core = tile.key.core();
            let bounds = tile.bitmap.bounds;
            canvas.save();
            // Draw only the core, sampling the two-pixel overlap at its edges.
            canvas.clip_rect(
                SkRect::new(
                    core.min.x as f32,
                    core.min.y as f32,
                    core.max.x as f32,
                    core.max.y as f32,
                ),
                None,
                false,
            );
            canvas.draw_image_rect_with_sampling_options(
                image,
                None,
                SkRect::new(
                    bounds.min.x as f32,
                    bounds.min.y as f32,
                    bounds.max.x as f32,
                    bounds.max.y as f32,
                ),
                SamplingOptions::from(FilterMode::Linear),
                &Paint::default(),
            );
            canvas.restore();
        }
    } else if scene.test_page {
        test_page::draw_contents(canvas, scene.source_page);
    }
    for stroke in scene.strokes.iter().chain(scene.demo_stroke.iter()) {
        draw_stroke(canvas, stroke);
    }
    if let Some(active) = &scene.active_stroke {
        draw_stroke(canvas, &active.record);
        draw_outline(canvas, &active.predicted_outline, &active.record.style);
    }
    canvas.restore();

    paint
        .set_style(Style::Stroke)
        .set_stroke_width(1.0 / scale)
        .set_color(Color::from_rgb(0xD9, 0xDC, 0xE2));
    canvas.draw_rect(paper, &paint);
    if scene.test_page {
        paint.set_color(Color::from_rgb(0x38, 0xA0, 0x73));
        let r = 5.0 / scale;
        canvas.draw_line((-r, -r), (r, r), &paint);
        canvas.draw_line((-r, r), (r, -r), &paint);
    }
    canvas.restore();
}

fn draw_stroke(canvas: &Canvas, stroke: &StrokeRecord) {
    draw_outline(canvas, &stroke.outline, &stroke.style);
}

fn draw_outline(canvas: &Canvas, outline: &[PathCommand], style: &PenStyle) {
    let mut builder = skia_safe::PathBuilder::new();
    let point = |p: Point| skia_safe::Point::new(p.x as f32, p.y as f32);
    for command in outline {
        match *command {
            PathCommand::MoveTo(p) => {
                builder.move_to(point(p));
            }
            PathCommand::LineTo(p) => {
                builder.line_to(point(p));
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                builder.cubic_to(point(control1), point(control2), point(to));
            }
            PathCommand::Close => {
                builder.close();
            }
        }
    }
    let mut paint = Paint::default();
    let [r, g, b] = style.color_rgb;
    paint
        .set_anti_alias(true)
        .set_color(Color::from_rgb(r, g, b));
    canvas.draw_path(&builder.detach(), &paint);
}
