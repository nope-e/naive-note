//! Paper frame and PDF preview composition; no PDF objects or parsing on the GL thread.

use skia_safe::{
    Canvas, Color, FilterMode, Image, Paint, Rect as SkRect, SamplingOptions, paint::Style,
};

use super::{PageScene, draft_paper};
use crate::document::{CanvasViewport, InkKind, PathCommand, PenStyle, Point, StrokeRecord};

pub(super) fn draw(
    canvas: &Canvas,
    scene: &PageScene,
    viewport: CanvasViewport,
    image: Option<&Image>,
    tile_images: &[Image],
    annotation_images: &mut super::annotations::AnnotationImages,
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
    }
    if scene.bitmap.is_none() {
        draft_paper::draw_contents(canvas, scene.source_page, scene.reference_lines);
    }
    for stroke in &scene.strokes {
        let transform = scene
            .selection_transform
            .filter(|_| scene.selected_ids.contains(&stroke.id));
        if let Some(transform) = transform {
            canvas.save();
            apply_transform(canvas, transform);
        }
        draw_stroke(canvas, stroke, annotation_images, scale * dpi);
        if transform.is_some() {
            canvas.restore();
        }
    }
    if let Some(preview) = &scene.selection_preview {
        canvas.save();
        apply_transform(canvas, preview.transform);
        for stroke in &preview.strokes {
            draw_stroke(canvas, stroke, annotation_images, scale * dpi);
        }
        canvas.restore();
    }
    if let Some(active) = &scene.active_stroke {
        // Fill confirmed ink and prediction together: translucent overlaps must
        // not darken the join within one highlighter object.
        draw_outline(
            canvas,
            active
                .record
                .outline
                .iter()
                .chain(active.predicted_outline.iter()),
            &active.record.style,
        );
    }
    super::selection::draw(canvas, scene, scale);
    canvas.restore();

    // Touching pages share this gray edge. Disable antialiasing so the shared
    // border stays a single solid logical pixel instead of blending twice.
    paint
        .set_anti_alias(false)
        .set_style(Style::Stroke)
        .set_stroke_width(1.0 / scale)
        .set_color(Color::from_rgb(0xB8, 0xBC, 0xC4));
    canvas.draw_rect(paper, &paint);
    if let Some(y) = scene.page_end_y {
        paint
            .set_anti_alias(true)
            .set_stroke_width(1.25 / scale)
            .set_color(Color::from_rgb(0xCD, 0x83, 0x34))
            .set_path_effect(skia_safe::PathEffect::dash(
                &[6.0 / scale, 4.0 / scale],
                0.0,
            ));
        canvas.draw_line((0.0, y as f32), (paper.width(), y as f32), &paint);
        paint.set_path_effect(None);
    }
    if scene.draft_paper {
        paint.set_stroke_width(1.0 / scale);
        paint.set_color(Color::from_rgb(0x38, 0xA0, 0x73));
        let r = 5.0 / scale;
        canvas.draw_line((-r, -r), (r, r), &paint);
        canvas.draw_line((-r, r), (r, -r), &paint);
    }
    canvas.restore();
}

fn draw_stroke(
    canvas: &Canvas,
    stroke: &StrokeRecord,
    annotation_images: &mut super::annotations::AnnotationImages,
    density: f32,
) {
    if stroke.pdf_annotation.is_some() {
        canvas.save();
        apply_transform(canvas, stroke.edit_transform);
        annotation_images.draw(canvas, stroke, density);
        canvas.restore();
        return;
    }
    draw_outline(canvas, stroke.outline.iter(), &stroke.style);
}

fn apply_transform(canvas: &Canvas, transform: crate::document::InkTransform) {
    canvas.translate((
        (transform.center.x + transform.translation.x) as f32,
        (transform.center.y + transform.translation.y) as f32,
    ));
    canvas.rotate(transform.radians.to_degrees() as f32, None);
    canvas.translate((-transform.center.x as f32, -transform.center.y as f32));
}

fn draw_outline<'a>(
    canvas: &Canvas,
    outline: impl IntoIterator<Item = &'a PathCommand>,
    style: &PenStyle,
) {
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
    if style.kind == InkKind::Highlighter {
        paint
            .set_alpha(102)
            .set_blend_mode(skia_safe::BlendMode::Multiply);
    }
    canvas.draw_path(&builder.detach(), &paint);
}
