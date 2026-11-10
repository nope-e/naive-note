//! Application lifecycle, window events and repaint scheduling.

use std::time::Instant;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow},
    keyboard::{KeyCode, PhysicalKey},
    window::WindowId,
};

use super::{Application, UserEvent, runtime::Runtime};

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
        match Runtime::new(event_loop, self.proxy.clone(), self.pen.clone()) {
            Ok(mut runtime) => {
                // Windows may defer RedrawRequested for a hidden window. Submit the
                // first frame synchronously before showing it, rather than waiting
                // for a redraw that itself depends on the window becoming visible.
                if let Err(error) = runtime.draw(event_loop, self.show_renderer_info) {
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
        runtime.drain_pen();
        if matches!(event, WindowEvent::Destroyed) {
            runtime.cancel_input();
            event_loop.exit();
            return;
        }
        if matches!(event, WindowEvent::CloseRequested) {
            runtime.cancel_input();
            runtime.canvas.request_exit();
            if runtime.canvas.take_exit_request() {
                event_loop.exit();
            } else {
                runtime.request_repaint();
            }
            return;
        }

        if runtime.handle_input(&event) {
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
                runtime.cancel_input();
                runtime.canvas.open_pdf(path);
                runtime.request_repaint();
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                // Coalesce size changes. GPU surfaces are resized to the latest
                // physical size only when a frame is actually drawn.
                runtime.request_repaint();
            }
            WindowEvent::Occluded(occluded) => {
                if occluded {
                    runtime.cancel_input();
                }
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
                if let Err(error) = runtime.draw(event_loop, self.show_renderer_info) {
                    self.fail(event_loop, error);
                    return;
                }
                if runtime.close_requested {
                    event_loop.exit();
                    return;
                }
            }
            _ => {}
        }
        if repaint && runtime.can_draw() {
            runtime.request_repaint();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Repaint(at) => {
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.frames.request_repaint(at);
                }
            }
            UserEvent::PdfResponse(response) => {
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.drain_pen();
                    runtime.canvas.pdf_response(*response);
                    if runtime.canvas.take_exit_request() {
                        event_loop.exit();
                        return;
                    }
                    runtime.request_repaint();
                }
            }
            UserEvent::StorageResponse(response) => {
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.canvas.storage_response(*response);
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
        runtime.drain_pen();
        let now = Instant::now();
        if runtime.canvas.maintenance(now) {
            runtime.request_repaint();
        }
        let autosave_at = runtime.canvas.autosave_deadline();
        event_loop.set_control_flow(ControlFlow::WaitUntil(autosave_at));
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
                event_loop.set_control_flow(ControlFlow::WaitUntil(at.min(autosave_at)));
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
