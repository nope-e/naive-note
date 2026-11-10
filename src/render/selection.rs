//! Selection guides are transient canvas overlays, never PDF content.

use skia_safe::{Canvas, Color, Paint, PathBuilder, Rect, paint::Style};

use super::PageScene;
use crate::document::Point;

pub(super) fn draw(canvas: &Canvas, scene: &PageScene, scale: f32) {
    let point = |p: Point| (p.x as f32, p.y as f32);
    let mut paint = Paint::default();
    paint
        .set_anti_alias(true)
        .set_style(Style::Stroke)
        .set_stroke_width(1.5 / scale)
        .set_color(Color::from_rgb(75, 165, 255));
    if let Some(overlay) = &scene.selection_overlay {
        let mut path = PathBuilder::new();
        path.move_to(point(overlay.corners[0]));
        for &p in &overlay.corners[1..] {
            path.line_to(point(p));
        }
        path.close();
        canvas.draw_path(&path.detach(), &paint);
        canvas.draw_line(
            point(overlay.rotation_anchor),
            point(overlay.rotation_handle),
            &paint,
        );
        paint.set_style(Style::Fill).set_color(Color::WHITE);
        canvas.draw_circle(point(overlay.rotation_handle), 6.0 / scale, &paint);
        paint
            .set_style(Style::Stroke)
            .set_color(Color::from_rgb(75, 165, 255));
        canvas.draw_circle(point(overlay.rotation_handle), 6.0 / scale, &paint);
    }
    if let Some(bounds) = scene.selection_rect {
        let rect = Rect::new(
            bounds.min.x as f32,
            bounds.min.y as f32,
            bounds.max.x as f32,
            bounds.max.y as f32,
        );
        canvas.draw_rect(rect, &paint);
        paint.set_style(Style::Fill).set_alpha(28);
        canvas.draw_rect(rect, &paint);
    }
    if let Some(&first) = scene.selection_path.first() {
        let mut path = PathBuilder::new();
        path.set_fill_type(skia_safe::PathFillType::EvenOdd);
        path.move_to(point(first));
        for &p in &scene.selection_path[1..] {
            path.line_to(point(p));
        }
        path.close();
        let path = path.detach();
        paint.set_style(Style::Fill).set_alpha(22);
        canvas.draw_path(&path, &paint);
        paint.set_style(Style::Stroke).set_alpha(255);
        // The closing edge makes the enclosure clear even before lifting.
        paint.set_path_effect(skia_safe::PathEffect::dash(
            &[5.0 / scale, 4.0 / scale],
            0.0,
        ));
        canvas.draw_path(&path, &paint);
    }
}
