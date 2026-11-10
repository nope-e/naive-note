//! Shared-context composition: UI layout -> Skia -> egui -> swap.

mod draft_paper;
mod fonts;
mod page;
mod skia;

use std::{sync::Arc, time::Instant};

use anyhow::{Result, anyhow};
use egui_glow::glow::{self, HasContext};
use winit::{
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::Window,
};

use crate::{
    app::UserEvent,
    document::{CanvasViewport, PageGeometry, Point, ViewTransform},
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
    pub strokes: Vec<Arc<crate::document::StrokeRecord>>,
    pub active_stroke: Option<crate::ink::ActiveInk>,
    pub selected_bounds: Vec<crate::document::Rect>,
    pub selection_rect: Option<crate::document::Rect>,
    pub draft_paper: bool,
    pub reference_lines: crate::document::ReferenceLines,
    pub page_end_y: Option<f64>,
}

pub struct Renderer {
    ui: EguiRenderer,
    skia: skia::SkiaRenderer,
    size: PhysicalSize<u32>,
}

impl Renderer {
    /// Floating chrome and popups take priority over the paper below them.
    pub fn ui_contains_point(&self, physical: Point) -> bool {
        let context = &self.ui.context;
        let scale = f64::from(context.pixels_per_point());
        egui::Popup::is_any_open(context)
            || context
                .layer_id_at(egui::pos2(
                    (physical.x / scale) as f32,
                    (physical.y / scale) as f32,
                ))
                .is_some_and(|layer| layer.order != egui::Order::Background)
    }

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
    pub fn pen_ui_event(&mut self, window: &Window, event: crate::platform::PenEvent) {
        use tablet_input_windows::PenPhase;
        let crate::platform::PenEvent { sample, modifiers } = event;
        if sample.phase == PenPhase::Cancel {
            self.cancel_pen_ui();
            return;
        }
        self.pen_pointer_event(window, event);
        if sample.phase == PenPhase::Leave {
            return;
        }
        let scale = self.ui.context.pixels_per_point();
        let pos = egui::pos2(
            sample.position[0] as f32 / scale,
            sample.position[1] as f32 / scale,
        );
        let input = self.ui.state.egui_input_mut();
        if matches!(sample.phase, PenPhase::Down | PenPhase::Up) {
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: sample.phase == PenPhase::Down,
                modifiers,
            });
        }
    }

    /// Canvas-owned pen samples still move the hover pointer, without sending
    /// buttons to egui or giving UI controls ownership of the stroke.
    pub fn pen_pointer_event(&mut self, window: &Window, event: crate::platform::PenEvent) {
        use tablet_input_windows::PenPhase;
        self.ui
            .state
            .egui_input_mut()
            .events
            .push(egui::Event::ModifiersChanged(event.modifiers));
        // Only egui-winit receives this virtual pointer event. Its cursor cache
        // must know the pointer is in the window even when promoted pen mouse
        // messages were filtered out. The device id is ignored by this path.
        let device_id = winit::event::DeviceId::dummy();
        let pointer = if matches!(event.sample.phase, PenPhase::Leave | PenPhase::Cancel) {
            WindowEvent::CursorLeft { device_id }
        } else {
            WindowEvent::CursorMoved {
                device_id,
                position: winit::dpi::PhysicalPosition::new(
                    event.sample.position[0],
                    event.sample.position[1],
                ),
            }
        };
        let _ = self.ui.state.on_window_event(window, &pointer);
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
        event_loop: &ActiveEventLoop,
        mut build_ui: impl FnMut(&mut egui::Ui) -> Option<CanvasScene>,
    ) -> Result<bool> {
        self.resize(window)?;
        egui_winit::update_viewport_info(
            self.ui
                .state
                .egui_input_mut()
                .viewports
                .entry(egui::ViewportId::ROOT)
                .or_default(),
            &self.ui.context,
            &window.window,
            false,
        );
        let input = self.ui.state.take_egui_input(&window.window);
        let mut scene = None;
        let mut output = self.ui.context.run_ui(input, |ui| scene = build_ui(ui));
        self.ui.state.handle_platform_output_with_event_loop(
            &window.window,
            event_loop,
            output.platform_output,
        );
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
        window.present()?;
        let mut close_requested = false;
        if let Some(viewport) = output.viewport_output.get_mut(&egui::ViewportId::ROOT) {
            for command in std::mem::take(&mut viewport.commands) {
                match command {
                    egui::ViewportCommand::Close => close_requested = true,
                    egui::ViewportCommand::Minimized(value) => window.window.set_minimized(value),
                    egui::ViewportCommand::Maximized(value) => window.window.set_maximized(value),
                    egui::ViewportCommand::StartDrag => {
                        if let Err(error) = window.window.drag_window() {
                            log::warn!("drag window: {error}");
                        }
                    }
                    egui::ViewportCommand::BeginResize(direction) => {
                        use egui::ResizeDirection as E;
                        use winit::window::ResizeDirection as W;
                        let direction = match direction {
                            E::North => W::North,
                            E::South => W::South,
                            E::East => W::East,
                            E::West => W::West,
                            E::NorthEast => W::NorthEast,
                            E::NorthWest => W::NorthWest,
                            E::SouthEast => W::SouthEast,
                            E::SouthWest => W::SouthWest,
                        };
                        if let Err(error) = window.window.drag_resize_window(direction) {
                            log::warn!("resize window: {error}");
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(close_requested)
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
        egui_extras::install_image_loaders(&context);
        fonts::configure(&context);
        crate::app::theme::configure(&context);
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
