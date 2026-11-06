use std::{
    collections::{HashMap, hash_map::Entry},
    sync::Arc,
};

use anyhow::{Context, Result};
use glutin::config::GlConfig;
use skia_safe::{
    AlphaType, Color, ColorType, Data, Image, ImageInfo, Surface,
    gpu::{self, DirectContext, SurfaceOrigin, gl::FramebufferInfo},
    images,
};
use winit::dpi::PhysicalSize;

use crate::{
    pdf::{PageBitmap, PageTile, TileKey},
    platform::GlWindow,
};

use super::{CanvasScene, page};

pub(super) struct SkiaRenderer {
    // The wrapped framebuffer must be released before its DirectContext.
    surface: Option<Surface>,
    page_images: HashMap<u32, (Arc<PageBitmap>, Image)>,
    tile_images: HashMap<TileKey, (Arc<PageTile>, Image)>,
    context: DirectContext,
    size: PhysicalSize<u32>,
    samples: usize,
    stencil_bits: usize,
}

impl SkiaRenderer {
    pub fn new(window: &GlWindow) -> Result<Self> {
        let interface =
            gpu::gl::Interface::new_load_with_cstr(|name| window.get_proc_address(name))
                .context("load Skia OpenGL interface")?;
        let mut context = gpu::direct_contexts::make_gl(interface, None)
            .context("create Skia Ganesh OpenGL context")?;
        context.set_resource_cache_limit(128 * 1024 * 1024);
        let mut renderer = Self {
            surface: None,
            page_images: HashMap::new(),
            tile_images: HashMap::new(),
            context,
            size: PhysicalSize::new(0, 0),
            samples: usize::from(window.config.num_samples()),
            stencil_bits: usize::from(window.config.stencil_size()),
        };
        renderer.resize(window.window.inner_size())?;
        log::info!("Skia Ganesh OpenGL renderer initialized");
        Ok(renderer)
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) -> Result<()> {
        if size == self.size {
            return Ok(());
        }
        // Release the old framebuffer wrapper before creating its replacement.
        self.surface = None;
        self.size = size;
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let dimensions = (i32::try_from(size.width)?, i32::try_from(size.height)?);
        let framebuffer = FramebufferInfo {
            fboid: 0, // The window's default framebuffer, shared with egui.
            format: gpu::gl::Format::RGBA8.into(),
            ..Default::default()
        };
        let target = gpu::backend_render_targets::make_gl(
            dimensions,
            self.samples,
            self.stencil_bits,
            framebuffer,
        );
        self.surface = Some(
            gpu::surfaces::wrap_backend_render_target(
                &mut self.context,
                &target,
                SurfaceOrigin::BottomLeft,
                ColorType::RGBA8888,
                None, // Untagged RGBA8: both renderers write into the same byte color space.
                None,
            )
            .context("wrap window framebuffer in a Skia surface")?,
        );
        log::debug!(
            "Skia window surface created: {} x {}",
            size.width,
            size.height
        );
        Ok(())
    }

    pub fn draw(&mut self, scene: Option<&CanvasScene>) -> Result<()> {
        let pages = scene.map_or(&[][..], |scene| scene.pages.as_slice());
        self.page_images.retain(|index, (cached, _)| {
            pages.iter().any(|page| {
                page.index == *index
                    && page
                        .bitmap
                        .as_ref()
                        .is_some_and(|bitmap| Arc::ptr_eq(cached, bitmap))
            })
        });
        for page in pages {
            if let Some(bitmap) = &page.bitmap
                && let Entry::Vacant(entry) = self.page_images.entry(page.index)
            {
                entry.insert((Arc::clone(bitmap), page_image(bitmap)?));
            }
        }
        // Retain only visible images. CPU pixels are shared with the LRU rather
        // than copied again into Skia; its GPU resource cache has a separate cap.
        let tiles: Vec<_> = pages.iter().flat_map(|page| &page.tiles).collect();
        self.tile_images.retain(|key, (cached, _)| {
            tiles
                .iter()
                .any(|tile| tile.key == *key && Arc::ptr_eq(tile, cached))
        });
        for tile in &tiles {
            if let Entry::Vacant(entry) = self.tile_images.entry(tile.key) {
                let image = page_image(&tile.bitmap)?;
                entry.insert((Arc::clone(tile), image));
            }
        }
        let Some(surface) = self.surface.as_mut() else {
            return Ok(());
        };
        // egui changed GL state in the previous frame. Invalidate Ganesh's cached state.
        self.context.reset(None);
        let canvas = surface.canvas();
        canvas.clear(Color::from_rgb(0xE7, 0xEC, 0xEA));
        if let Some(scene) = scene {
            for page in &scene.pages {
                let tile_images: Vec<_> = page
                    .tiles
                    .iter()
                    .map(|tile| self.tile_images[&tile.key].1.clone())
                    .collect();
                page::draw(
                    canvas,
                    page,
                    scene.viewport,
                    self.page_images.get(&page.index).map(|(_, image)| image),
                    &tile_images,
                );
            }
        }

        self.context
            .flush_and_submit()
            .context("flush Skia drawing")?;
        self.context.reset_gl_texture_bindings();
        Ok(())
    }
}

impl Drop for SkiaRenderer {
    fn drop(&mut self) {
        self.surface = None;
        self.page_images.clear();
        self.tile_images.clear();
        // Release while the GL context is still current, including on initialization errors.
        // This also follows rust-skia's recommended shutdown for AMD drivers.
        self.context.release_resources_and_abandon();
    }
}

fn page_image(bitmap: &PageBitmap) -> Result<Image> {
    let info = ImageInfo::new(
        (i32::try_from(bitmap.width)?, i32::try_from(bitmap.height)?),
        ColorType::RGBA8888,
        AlphaType::Opaque,
        None,
    );
    images::raster_from_data(
        &info,
        Data::new_with_owner(Arc::clone(&bitmap.rgba)),
        bitmap.width as usize * 4,
    )
    .context("Create Skia PDF page image")
}
