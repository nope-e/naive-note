//! Native window and OpenGL context ownership. All access stays on the UI thread.

use std::{ffi::CStr, num::NonZeroU32, sync::Arc};

use anyhow::{Context, Result, anyhow};
use egui_glow::glow::{self, HasContext};
use glutin::{
    config::{Config, ConfigTemplateBuilder, GlConfig},
    context::{ContextApi, ContextAttributesBuilder, GlProfile, PossiblyCurrentContext, Version},
    display::{GetGlDisplay, GlDisplay},
    prelude::{GlSurface, NotCurrentGlContext},
    surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface},
};
use raw_window_handle::HasWindowHandle;
use winit::{dpi::LogicalSize, event_loop::ActiveEventLoop, window::Window};

/// Renderers must be dropped before this owner; the native window is dropped last.
pub struct GlWindow {
    pub gl: Arc<glow::Context>,
    context: PossiblyCurrentContext,
    surface: Surface<WindowSurface>,
    pub config: Config,
    pub window: Window,
}

impl GlWindow {
    pub fn new(event_loop: &ActiveEventLoop) -> Result<Self> {
        let attributes = Window::default_attributes()
            .with_title("Naive Note")
            .with_inner_size(LogicalSize::new(1100.0, 780.0))
            .with_min_inner_size(LogicalSize::new(320.0, 240.0))
            .with_visible(false);
        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_depth_size(0)
            .with_stencil_size(8)
            .with_transparency(false);
        let (window, config) = glutin_winit::DisplayBuilder::new()
            .with_window_attributes(Some(attributes))
            .build(event_loop, template, |configs| {
                // Skia handles path antialiasing; avoid requiring a multisampled window.
                configs
                    .min_by_key(|config| config.num_samples())
                    .expect("no matching OpenGL window configuration")
            })
            .map_err(|error| anyhow!("create OpenGL window: {error}"))?;
        let window = window.context("OpenGL display did not create a window")?;
        let handle = window.window_handle()?.as_raw();
        let display = config.display();
        let context_attributes = ContextAttributesBuilder::new()
            .with_profile(GlProfile::Core)
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(Some(handle));
        // SAFETY: The handle belongs to `window`, which outlives the context and surface.
        let context = unsafe { display.create_context(&config, &context_attributes) }
            .context("create OpenGL 3.3 core context; check the graphics driver")?;
        let size = window.inner_size();
        let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new()
            .with_srgb(Some(false))
            .build(handle, nonzero(size.width), nonzero(size.height));
        // SAFETY: The window handle and matching display/config remain alive in this owner.
        let surface = unsafe { display.create_window_surface(&config, &surface_attributes) }
            .context("create OpenGL window surface")?;
        let context = context
            .make_current(&surface)
            .context("make OpenGL context current")?;
        if let Err(error) = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::MIN))
        {
            log::warn!("could not enable vsync: {error}");
        }
        // SAFETY: The matching GL context is current on this thread before loading functions.
        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|name| display.get_proc_address(name))
        };
        // SAFETY: The context is current and these queries do not change resource ownership.
        unsafe {
            log::info!(
                "OpenGL {} | {} | {}",
                gl.get_parameter_string(glow::VERSION),
                gl.get_parameter_string(glow::VENDOR),
                gl.get_parameter_string(glow::RENDERER),
            );
        }
        Ok(Self {
            gl: Arc::new(gl),
            context,
            surface,
            config,
            window,
        })
    }

    pub fn get_proc_address(&self, name: &CStr) -> *const std::ffi::c_void {
        self.config.display().get_proc_address(name)
    }

    pub fn resize(&self, size: winit::dpi::PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            self.surface
                .resize(&self.context, nonzero(size.width), nonzero(size.height));
        }
    }

    pub fn present(&self) -> Result<()> {
        self.window.pre_present_notify();
        self.surface
            .swap_buffers(&self.context)
            .context("swap OpenGL buffers")
    }
}

fn nonzero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap_or(NonZeroU32::MIN)
}
