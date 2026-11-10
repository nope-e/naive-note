//! Rasterize visible annotation SVGs at the current display density, with a byte budget.

use std::{collections::HashSet, sync::Arc};

use anyhow::{Context, Result, anyhow};
use skia_safe::{
    AlphaType, Canvas, ColorType, Data, Image, ImageInfo, Paint, QuickReject, Rect, images,
};
use uuid::Uuid;

use crate::{document::StrokeRecord, pdf::ByteLru};

pub(super) struct AnnotationImages {
    images: ByteLru<(Uuid, u32, u32), Image>,
    failed: HashSet<Uuid>,
}

impl Default for AnnotationImages {
    fn default() -> Self {
        Self {
            images: ByteLru::new(32 * 1024 * 1024),
            failed: HashSet::new(),
        }
    }
}

impl AnnotationImages {
    pub fn draw(&mut self, canvas: &Canvas, stroke: &StrokeRecord, density: f32) {
        let Some(source) = &stroke.pdf_annotation else {
            return;
        };
        let bounds = source.bounds;
        let rect = Rect::new(
            bounds.min.x as f32,
            bounds.min.y as f32,
            bounds.max.x as f32,
            bounds.max.y as f32,
        );
        if canvas.quick_reject(&rect) || self.failed.contains(&stroke.id) {
            return;
        }
        let (width, height) =
            raster_size(bounds.size().width, bounds.size().height, density as f64);
        let key = (stroke.id, width, height);
        let image = if let Some(image) = self.images.get(&key) {
            image
        } else {
            match rasterize(&source.svg, width, height) {
                Ok(image) => {
                    let image = Arc::new(image);
                    self.images.insert(key, Arc::clone(&image), |_| {
                        width as usize * height as usize * 4
                    });
                    image
                }
                Err(error) => {
                    log::warn!("Cannot draw standard annotation: {error:#}");
                    self.failed.insert(stroke.id);
                    return;
                }
            }
        };
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        if source.multiply {
            paint.set_blend_mode(skia_safe::BlendMode::Multiply);
        }
        canvas.draw_image_rect_with_sampling_options(
            &*image,
            None,
            rect,
            skia_safe::SamplingOptions::from(skia_safe::FilterMode::Linear),
            &paint,
        );
    }
}

fn raster_size(width: f64, height: f64, density: f64) -> (u32, u32) {
    let density = 2.0_f64.powf((density.max(0.01).log2() * 2.).ceil() / 2.);
    let scale = density
        .min(4096. / width.max(height))
        .min((4_194_304. / (width * height)).sqrt());
    (
        (width * scale).ceil().clamp(1., 4096.) as u32,
        (height * scale).ceil().clamp(1., 4096.) as u32,
    )
}

fn rasterize(svg: &str, width: u32, height: u32) -> Result<Image> {
    let image = egui_extras::image::load_svg_bytes_with_size(
        svg.as_bytes(),
        egui::SizeHint::Size {
            width,
            height,
            maintain_aspect_ratio: false,
        },
        &Default::default(),
    )
    .map_err(|error| anyhow!(error))?;
    let pixels = image
        .pixels
        .iter()
        .flat_map(|color| color.to_array())
        .collect::<Vec<_>>();
    let info = ImageInfo::new(
        (width as i32, height as i32),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    images::raster_from_data(&info, Data::new_with_owner(pixels), width as usize * 4)
        .context("Create standard annotation image")
}
