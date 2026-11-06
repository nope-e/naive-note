use std::sync::Arc;

use anyhow::{Context, Result};
use glutin::config::GlConfig;
use skia_safe::{
    AlphaType, Color, ColorType, Data, Image, ImageInfo, Surface,
    gpu::{self, DirectContext, SurfaceOrigin, gl::FramebufferInfo},
    images,
};
use winit::dpi::PhysicalSize;

use crate::{pdf::PageBitmap, platform::GlWindow};

use super::{CanvasScene, page};

pub(super) struct SkiaRenderer {
    // The wrapped framebuffer must be released before its DirectContext.
    surface: Option<Surface>,
    page_image: Option<(Arc<PageBitmap>, Image)>,
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
        let context = gpu::direct_contexts::make_gl(interface, None)
            .context("create Skia Ganesh OpenGL context")?;
        let mut renderer = Self {
            surface: None,
            page_image: None,
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
        log::info!(
            "Skia window surface created: {} x {}",
            size.width,
            size.height
        );
        Ok(())
    }

    pub fn draw(&mut self, scene: Option<&CanvasScene>) -> Result<()> {
        let bitmap = scene.and_then(|scene| scene.bitmap.as_ref());
        if let Some(bitmap) = bitmap {
            if !self
                .page_image
                .as_ref()
                .is_some_and(|(cached, _)| Arc::ptr_eq(cached, bitmap))
            {
                let info = ImageInfo::new(
                    (i32::try_from(bitmap.width)?, i32::try_from(bitmap.height)?),
                    ColorType::RGBA8888,
                    AlphaType::Opaque,
                    None,
                );
                let image = images::raster_from_data(
                    &info,
                    Data::new_copy(&bitmap.rgba),
                    bitmap.width as usize * 4,
                )
                .context("Create Skia PDF page image")?;
                self.page_image = Some((Arc::clone(bitmap), image));
            }
        } else {
            self.page_image = None;
        }
        let Some(surface) = self.surface.as_mut() else {
            return Ok(());
        };
        // egui changed GL state in the previous frame. Invalidate Ganesh's cached state.
        self.context.reset(None);
        let canvas = surface.canvas();
        canvas.clear(Color::from_rgb(0xE7, 0xEC, 0xEA));
        if let Some(scene) = scene {
            page::draw(
                canvas,
                scene,
                self.page_image.as_ref().map(|(_, image)| image),
            );
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
        self.page_image = None;
        // Release while the GL context is still current, including on initialization errors.
        // This also follows rust-skia's recommended shutdown for AMD drivers.
        self.context.release_resources_and_abandon();
    }
}
