//! Small, outlined cursor bitmaps with the hotspot at the exact drawing position.

use std::collections::HashMap;

use super::{CanvasCursor, CursorShape, CursorStyle};

#[derive(Default)]
pub(crate) struct CursorImages {
    scale: f64,
    images: HashMap<CursorStyle, egui::CustomCursorImage>,
    eraser: Option<egui::CustomCursorImage>,
}

impl CursorImages {
    pub fn apply(&mut self, context: &egui::Context, cursor: CanvasCursor) {
        self.set_scale(f64::from(context.pixels_per_point()));
        let (icon, image) = match cursor {
            CanvasCursor::Icon(icon) => (icon, None),
            CanvasCursor::Style(style) => (egui::CursorIcon::Default, self.get(style)),
            CanvasCursor::Eraser => (egui::CursorIcon::Crosshair, Some(self.eraser())),
        };
        context.set_cursor_icon(icon);
        context.set_cursor_image(image);
    }

    fn set_scale(&mut self, scale: f64) {
        let scale = scale.clamp(0.5, 8.0);
        if self.scale != scale {
            self.scale = scale;
            self.images.clear();
            self.eraser = None;
        }
    }

    fn get(&mut self, style: CursorStyle) -> Option<egui::CustomCursorImage> {
        (style.shape != CursorShape::System).then(|| {
            self.images
                .entry(style)
                .or_insert_with(|| {
                    let color = if style.shape == CursorShape::Dot {
                        [230, 45, 45]
                    } else {
                        [40, 40, 40]
                    };
                    image(self.scale, color, |x, y| {
                        distance(style.shape, style.size.radius(), x, y)
                    })
                })
                .clone()
        })
    }

    fn eraser(&mut self) -> egui::CustomCursorImage {
        self.eraser
            .get_or_insert_with(|| {
                image(self.scale, [40, 40, 40], |x, y| {
                    (x.hypot(y) - 8.0).abs() - 0.55
                })
            })
            .clone()
    }
}

fn image(
    scale: f64,
    foreground: [u8; 3],
    distance_at: impl Fn(f64, f64) -> f64,
) -> egui::CustomCursorImage {
    let hotspot = (24.0 * scale).ceil() as u16;
    let side = 2 * hotspot + 1;
    let mut rgba = Vec::with_capacity(usize::from(side).pow(2) * 4);
    for y in 0..side {
        for x in 0..side {
            let mut alpha = 0.0;
            let mut rgb = [0.0; 3];
            // Four subpixel samples keep thin cursors legible at fractional DPI.
            for (dx, dy) in [(-0.25, -0.25), (0.25, -0.25), (-0.25, 0.25), (0.25, 0.25)] {
                let px = (f64::from(x) - f64::from(hotspot) + dx) / scale;
                let py = (f64::from(y) - f64::from(hotspot) + dy) / scale;
                let distance = distance_at(px, py);
                let coverage = (0.5 - (distance - 0.9) * scale).clamp(0.0, 1.0);
                let fill = (0.5 - distance * scale).clamp(0.0, 1.0);
                alpha += coverage;
                for (channel, foreground) in rgb.iter_mut().zip(foreground) {
                    *channel += coverage * (245.0 + (f64::from(foreground) - 245.0) * fill);
                }
            }
            let [red, green, blue] = rgb.map(|channel| {
                if alpha > 0.0 {
                    (channel / alpha).round() as u8
                } else {
                    0
                }
            });
            rgba.extend_from_slice(&[red, green, blue, (alpha * 255.0 / 4.0).round() as u8]);
        }
    }
    egui::CustomCursorImage {
        rgba: rgba.into(),
        size: [side, side],
        hotspot: [hotspot, hotspot],
    }
}

fn distance(shape: CursorShape, r: f64, x: f64, y: f64) -> f64 {
    match shape {
        CursorShape::System => unreachable!("system cursors have no bitmap"),
        CursorShape::Dot => x.hypot(y) - r,
        CursorShape::Crosshair => {
            segment(x, y, [-r * 1.7, 0.0], [r * 1.7, 0.0]).min(segment(
                x,
                y,
                [0.0, -r * 1.7],
                [0.0, r * 1.7],
            )) - 0.55
        }
        CursorShape::Beam => {
            segment(x, y, [0.0, -r * 1.7], [0.0, r * 1.7])
                .min(segment(x, y, [-r * 0.7, -r * 1.7], [r * 0.7, -r * 1.7]))
                .min(segment(x, y, [-r * 0.7, r * 1.7], [r * 0.7, r * 1.7]))
                - 0.55
        }
        CursorShape::TeardropNorthWest
        | CursorShape::TeardropNorth
        | CursorShape::TeardropNorthEast => {
            let (x, y) = match shape {
                CursorShape::TeardropNorthWest => (
                    (x - y) * std::f64::consts::FRAC_1_SQRT_2,
                    (x + y) * std::f64::consts::FRAC_1_SQRT_2,
                ),
                CursorShape::TeardropNorthEast => (
                    (x + y) * std::f64::consts::FRAC_1_SQRT_2,
                    (y - x) * std::f64::consts::FRAC_1_SQRT_2,
                ),
                _ => (x, y),
            };
            // A rounded bulb joined to a pointed tip at the hotspot.
            let bulb = x.hypot(y - r * 2.0) - r;
            let edge = segment(x, y, [0.0, 0.0], [r * 0.94, r * 1.66])
                .min(segment(x, y, [r * 0.94, r * 1.66], [-r * 0.94, r * 1.66]))
                .min(segment(x, y, [-r * 0.94, r * 1.66], [0.0, 0.0]));
            let inside = (0.0..=r * 1.66).contains(&y) && x.abs() <= y * 0.94 / 1.66;
            bulb.min(if inside { -edge } else { edge })
        }
    }
}

fn segment(x: f64, y: f64, a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let t = (((x - a[0]) * dx + (y - a[1]) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (x - a[0] - t * dx).hypot(y - a[1] - t * dy)
}
