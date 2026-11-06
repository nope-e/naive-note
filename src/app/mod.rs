//! Window lifecycle, event routing and the application's egui layout.

mod canvas;
mod frame_rate;

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
    PdfOpened(Box<crate::pdf::OpenResponse>),
}

pub fn run() -> Result<()> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let mut app = Application {
        runtime: None,
        proxy: event_loop.create_proxy(),
        repaint_at: None,
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
    repaint_at: Option<Instant>,
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
        // egui may repeat layout with consumed input. Keep the updated state,
        // but process input only on the first pass of this frame.
        let mut process_input = true;
        let size = self.window.window.inner_size();
        if show_renderer_info {
            self.frame_rate.begin_frame();
        }
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
            self.window.window.request_redraw();
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
            self.window.window.request_redraw();
        }
        Ok(())
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
        if let Some(runtime) = &self.runtime {
            runtime.window.window.request_redraw();
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
                runtime.window.window.request_redraw();
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
            .on_window_event(&runtime.window.window, &event);
        match event {
            WindowEvent::DroppedFile(path) => {
                runtime.canvas.open_pdf(path);
                runtime.window.window.request_redraw();
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Err(error) = runtime.renderer.resize(&runtime.window) {
                    self.fail(event_loop, error);
                    return;
                }
                runtime.window.window.request_redraw();
            }
            WindowEvent::Occluded(occluded) => {
                runtime.occluded = occluded;
                if !occluded {
                    runtime.window.window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && !event.repeat
                    && event.physical_key == PhysicalKey::Code(KeyCode::F12) =>
            {
                self.show_renderer_info = !self.show_renderer_info;
                runtime.frame_rate.reset();
                runtime.window.window.request_redraw();
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
            runtime.window.window.request_redraw();
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Repaint(at) => {
                self.repaint_at = Some(self.repaint_at.map_or(at, |pending| pending.min(at)));
            }
            UserEvent::PdfOpened(response) => {
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.canvas.pdf_opened(*response);
                    runtime.window.window.request_redraw();
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
            self.repaint_at = None;
            return;
        }
        if let Some(at) = self.repaint_at {
            if at <= Instant::now() {
                self.repaint_at = None;
                runtime.window.window.request_redraw();
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(at));
            }
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.runtime = None;
        self.repaint_at = None;
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.runtime = None;
    }
}
