//! Window lifecycle, event routing and the application's egui layout.

mod canvas;
mod frame_rate;
mod frame_scheduler;

use std::{path::PathBuf, time::Instant};

use anyhow::{Context, Result};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{KeyCode, PhysicalKey},
    window::WindowId,
};

use crate::{platform::GlWindow, render::Renderer};

pub enum UserEvent {
    Repaint(Instant),
    PdfResponse(Box<crate::pdf::PdfResponse>),
}

pub fn run() -> Result<()> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let mut app = Application {
        runtime: None,
        proxy: event_loop.create_proxy(),
        error: None,
        show_renderer_info: false,
    };
    event_loop
        .run_app(&mut app)
        .context("run window event loop")?;
    match app.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct Application {
    runtime: Option<Runtime>,
    proxy: EventLoopProxy<UserEvent>,
    error: Option<anyhow::Error>,
    show_renderer_info: bool,
}

struct Runtime {
    // Rust drops fields in declaration order: all GPU renderers before the native context.
    renderer: Renderer,
    window: GlWindow,
    occluded: bool,
    canvas: canvas::CanvasWorkspace,
    frame_rate: frame_rate::FrameRate,
    frames: frame_scheduler::FrameScheduler,
}

impl Runtime {
    fn new(event_loop: &ActiveEventLoop, proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        let window = GlWindow::new(event_loop)?;
        let renderer = Renderer::new(&window, proxy.clone())?;
        let mut canvas = canvas::CanvasWorkspace::new(proxy)?;
        if let Some(path) = std::env::args_os().nth(1) {
            canvas.open_pdf(PathBuf::from(path));
        }
        Ok(Self {
            renderer,
            window,
            occluded: false,
            canvas,
            frame_rate: frame_rate::FrameRate::default(),
            frames: frame_scheduler::FrameScheduler::default(),
        })
    }

    fn can_draw(&self) -> bool {
        let size = self.window.window.inner_size();
        size.width > 0
            && size.height > 0
            && !self.occluded
            && !self.window.window.is_minimized().unwrap_or(false)
    }

    fn draw(&mut self, show_renderer_info: bool) -> Result<()> {
        if !self.frames.begin_frame(Instant::now()) {
            return Ok(());
        }
        // egui may repeat layout with consumed input. Keep the updated state,
        // but process input only on the first pass of this frame.
        let mut process_input = true;
        let size = self.window.window.inner_size();
        let fps = self.frame_rate.fps();
        self.renderer.draw(&self.window, |ui| {
            let scene = self.canvas.show(
                ui,
                show_renderer_info,
                fps,
                (size.width, size.height),
                process_input,
            );
            process_input = false;
            scene
        })?;
        if show_renderer_info {
            self.frame_rate.frame_presented();
            self.request_repaint();
        }
        // Open the native dialog once, after egui's possibly repeated layout.
        if self.canvas.take_open_request() {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Open PDF")
                .add_filter("PDF document", &["pdf"])
                .set_parent(&self.window.window)
                .pick_file()
            {
                self.canvas.open_pdf(path);
            }
            self.frame_rate.reset();
            self.request_repaint();
        }
        Ok(())
    }

    fn request_repaint(&mut self) {
        self.frames.request_repaint(Instant::now());
    }
}

impl Application {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        log::error!("{error:#}");
        self.error = Some(error);
        event_loop.exit();
    }
}

impl ApplicationHandler<UserEvent> for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(runtime) = &mut self.runtime {
            runtime.request_repaint();
            return;
        }
        match Runtime::new(event_loop, self.proxy.clone()) {
            Ok(mut runtime) => {
                // Windows may defer RedrawRequested for a hidden window. Submit the
                // first frame synchronously before showing it, rather than waiting
                // for a redraw that itself depends on the window becoming visible.
                if let Err(error) = runtime.draw(self.show_renderer_info) {
                    self.fail(event_loop, error);
                    return;
                }
                runtime.window.window.set_visible(true);
                log::info!("first frame presented; window shown");
                runtime.request_repaint();
                self.runtime = Some(runtime);
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        if runtime.window.window.id() != id {
            return;
        }
        if matches!(event, WindowEvent::CloseRequested) {
            event_loop.exit();
            return;
        }

        let repaint = runtime
            .renderer
            .on_window_event(&runtime.window.window, &event)
            // egui marks redraws and moves as repaint-worthy. Re-arming a redraw
            // here would create a perpetual WM_PAINT loop, even without F12.
            // Moving only changes desktop position; DPI/size have their own events.
            && !matches!(event, WindowEvent::RedrawRequested | WindowEvent::Moved(_));
        match event {
            WindowEvent::DroppedFile(path) => {
                runtime.canvas.open_pdf(path);
                runtime.request_repaint();
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                // Coalesce size changes. GPU surfaces are resized to the latest
                // physical size only when a frame is actually drawn.
                runtime.request_repaint();
            }
            WindowEvent::Occluded(occluded) => {
                runtime.occluded = occluded;
                if !occluded {
                    runtime.request_repaint();
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && !event.repeat
                    && event.physical_key == PhysicalKey::Code(KeyCode::F12) =>
            {
                self.show_renderer_info = !self.show_renderer_info;
                runtime.frame_rate.reset();
                runtime.request_repaint();
            }
            WindowEvent::RedrawRequested if runtime.can_draw() => {
                if let Err(error) = runtime.draw(self.show_renderer_info) {
                    self.fail(event_loop, error);
                    return;
                }
            }
            _ => {}
        }
        if repaint && runtime.can_draw() {
            runtime.request_repaint();
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Repaint(at) => {
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.frames.request_repaint(at);
                }
            }
            UserEvent::PdfResponse(response) => {
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.canvas.pdf_response(*response);
                    runtime.request_repaint();
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
        let Some(runtime) = &mut self.runtime else {
            return;
        };
        if !runtime.can_draw() {
            runtime.frame_rate.reset();
            runtime.frames.cancel_repaint();
            return;
        }
        if let Some(at) = runtime.frames.next_repaint() {
            if at <= Instant::now() {
                // Request only from the outer loop, never chain WM_PAINT from
                // inside draw: Windows uses the UI thread for native move/resize.
                runtime.window.window.request_redraw();
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(at));
            }
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.runtime = None;
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.runtime = None;
    }
}
