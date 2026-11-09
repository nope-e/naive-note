//! Shared-context composition: UI layout -> Skia -> egui -> swap.

mod fonts;
mod page;
mod skia;
mod test_page;

use std::{sync::Arc, time::Instant};

use anyhow::{Result, anyhow};
use egui_glow::glow::{self, HasContext};
use winit::{dpi::PhysicalSize, event::WindowEvent, event_loop::EventLoopProxy, window::Window};

use crate::{
    app::UserEvent,
    document::{CanvasViewport, PageGeometry, ViewTransform},
    pdf::{PageBitmap, PageTile},
    platform::GlWindow,
};

#[derive(Clone)]
pub struct CanvasScene {
    pub viewport: CanvasViewport,
    pub pages: Vec<PageScene>,
}

#[derive(Clone)]
pub struct PageScene {
    pub index: u32,
    pub page: PageGeometry,
    pub source_page: PageGeometry,
    pub view: ViewTransform,
    pub bitmap: Option<Arc<PageBitmap>>,
    pub tiles: Vec<Arc<PageTile>>,
    pub demo_stroke: Option<Arc<crate::document::StrokeRecord>>,
    pub strokes: Vec<Arc<crate::document::StrokeRecord>>,
    pub active_stroke: Option<crate::ink::ActiveInk>,
    pub test_page: bool,
}

pub struct Renderer {
    ui: EguiRenderer,
    skia: skia::SkiaRenderer,
    size: PhysicalSize<u32>,
}

impl Renderer {
    pub fn new(window: &GlWindow, proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        let skia = skia::SkiaRenderer::new(window)?;
        let ui = EguiRenderer::new(window, proxy)?;
        Ok(Self {
            ui,
            skia,
            size: window.window.inner_size(),
        })
    }

    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        self.ui.state.on_window_event(window, event).repaint
    }

    /// Native pen contacts outside the canvas operate UI controls exactly once.
    pub fn pen_ui_event(&mut self, event: crate::platform::PenEvent) {
        use tablet_input_windows::PenPhase;
        let crate::platform::PenEvent { sample, modifiers } = event;
        if sample.phase == PenPhase::Cancel {
            self.cancel_pen_ui();
            return;
        }
        let scale = self.ui.context.pixels_per_point();
        let pos = egui::pos2(
            sample.position[0] as f32 / scale,
            sample.position[1] as f32 / scale,
        );
        let input = self.ui.state.egui_input_mut();
        input.events.push(egui::Event::ModifiersChanged(modifiers));
        if sample.phase == PenPhase::Leave {
            input.events.push(egui::Event::PointerGone);
            return;
        }
        input.events.push(egui::Event::PointerMoved(pos));
        if matches!(sample.phase, PenPhase::Down | PenPhase::Up) {
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: sample.phase == PenPhase::Down,
                modifiers,
            });
        }
    }

    pub fn cancel_pen_ui(&mut self) {
        let modifiers = self.ui.context.input(|input| input.modifiers);
        let input = self.ui.state.egui_input_mut();
        let pos = egui::pos2(-1e6, -1e6);
        input.events.push(egui::Event::PointerMoved(pos));
        input.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        });
        input.events.push(egui::Event::PointerGone);
    }

    fn resize(&mut self, window: &GlWindow) -> Result<()> {
        let size = window.window.inner_size();
        if size != self.size {
            window.resize(size);
            self.skia.resize(size)?;
            self.size = size;
        }
        Ok(())
    }

    pub fn draw(
        &mut self,
        window: &GlWindow,
        mut build_ui: impl FnMut(&mut egui::Ui) -> Option<CanvasScene>,
    ) -> Result<()> {
        self.resize(window)?;
        let input = self.ui.state.take_egui_input(&window.window);
        let mut scene = None;
        let mut output = self.ui.context.run_ui(input, |ui| scene = build_ui(ui));
        self.ui
            .state
            .handle_platform_output(&window.window, output.platform_output);
        let primitives = self
            .ui
            .context
            .tessellate(output.shapes, output.pixels_per_point);

        // SAFETY: The GL context is current on this thread. Framebuffer 0 is the native
        // drawable used by both renderers, with framebuffer sRGB conversion disabled.
        unsafe {
            window.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            window.gl.disable(glow::FRAMEBUFFER_SRGB);
        }
        self.skia.draw(scene.as_ref())?;

        // SAFETY: Skia has flushed. Restore the upload/draw state not owned by egui's
        // painter, before it uploads textures and configures its own pipeline.
        unsafe {
            window.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            window.gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
            window.gl.disable(glow::DEPTH_TEST);
            window.gl.disable(glow::STENCIL_TEST);
            window.gl.color_mask(true, true, true, true);
        }
        self.ui.painter.paint_and_update_textures(
            window.window.inner_size().into(),
            output.pixels_per_point,
            &primitives,
            &mut output.textures_delta,
        );
        window.present()
    }
}

struct EguiRenderer {
    context: egui::Context,
    state: egui_winit::State,
    painter: egui_glow::Painter,
}

impl EguiRenderer {
    fn new(window: &GlWindow, proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        let context = egui::Context::default();
        fonts::configure(&context);
        context.set_visuals(egui::Visuals::light());
        context.options_mut(|options| options.zoom_with_keyboard = false);
        context.set_request_repaint_callback(move |request| {
            if let Some(at) = Instant::now().checked_add(request.delay) {
                let _ = proxy.send_event(UserEvent::Repaint(at));
            }
        });
        let painter = egui_glow::Painter::new(Arc::clone(&window.gl), "", None, false)
            .map_err(|error| anyhow!("create egui OpenGL painter: {error}"))?;
        let state = egui_winit::State::new(
            context.clone(),
            egui::ViewportId::ROOT,
            &window.window,
            Some(window.window.scale_factor() as f32),
            window.window.theme(),
            Some(painter.max_texture_side()),
        );
        Ok(Self {
            context,
            state,
            painter,
        })
    }
}

impl Drop for EguiRenderer {
    fn drop(&mut self) {
        self.painter.destroy();
    }
}
