//! Window-owned resources, frame drawing and native dialog execution.

mod input;

use std::time::Instant;

use anyhow::Result;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};

use crate::{
    document::Point,
    platform::{GlWindow, PenInput},
    render::Renderer,
};

use super::{UserEvent, canvas, frame_rate, frame_scheduler};

pub(super) struct Runtime {
    // Rust drops fields in declaration order: all GPU renderers before the native context.
    pub(super) renderer: Renderer,
    // Detach the native adapter before dropping its HWND.
    pen: PenInput,
    pub(super) window: GlWindow,
    pub(super) occluded: bool,
    pub(super) close_requested: bool,
    pub(super) canvas: canvas::CanvasWorkspace,
    pub(super) frame_rate: frame_rate::FrameRate,
    pub(super) frames: frame_scheduler::FrameScheduler,
    mouse_position: Option<Point>,
    mouse_owned: bool,
    modifiers: egui::Modifiers,
}

impl Runtime {
    pub(super) fn new(
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<UserEvent>,
        pen: PenInput,
    ) -> Result<Self> {
        let window = GlWindow::new(event_loop)?;
        let renderer = Renderer::new(&window, proxy.clone())?;
        let canvas = canvas::CanvasWorkspace::new(proxy)?;
        pen.attach(&window.window)?;
        Ok(Self {
            renderer,
            pen,
            window,
            occluded: false,
            close_requested: false,
            canvas,
            frame_rate: frame_rate::FrameRate::default(),
            frames: frame_scheduler::FrameScheduler::default(),
            mouse_position: None,
            mouse_owned: false,
            modifiers: egui::Modifiers::NONE,
        })
    }

    pub(super) fn can_draw(&self) -> bool {
        let size = self.window.window.inner_size();
        size.width > 0
            && size.height > 0
            && !self.occluded
            && !self.window.window.is_minimized().unwrap_or(false)
    }

    pub(super) fn draw(
        &mut self,
        event_loop: &ActiveEventLoop,
        show_renderer_info: bool,
    ) -> Result<()> {
        if !self.frames.begin_frame(Instant::now()) {
            return Ok(());
        }
        // egui may repeat layout with consumed input. Keep the updated state,
        // but process input only on the first pass of this frame.
        let mut process_input = true;
        let size = self.window.window.inner_size();
        let fps = self.frame_rate.fps();
        let close_requested = self.renderer.draw(&self.window, event_loop, |ui| {
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
        if close_requested {
            self.cancel_input();
            self.canvas.request_exit();
            self.request_repaint();
        }
        self.close_requested = self.canvas.take_exit_request();
        if show_renderer_info {
            self.frame_rate.frame_presented();
            self.request_repaint();
        }
        // Open the native dialog once, after egui's possibly repeated layout.
        if self.canvas.take_open_request() {
            self.cancel_input();
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Open PDF")
                .add_filter("PDF document", &["pdf"])
                .set_parent(&self.window.window)
                .pick_file()
            {
                self.canvas.open_pdf(path);
            }
            // Discard any contacts queued while the modal dialog was open.
            self.cancel_input();
            self.frame_rate.reset();
            self.request_repaint();
        }
        if let Some(suggested) = self.canvas.take_save_request() {
            self.cancel_input();
            let mut dialog = rfd::FileDialog::new()
                .set_title("另存为 PDF")
                .add_filter("PDF document", &["pdf"])
                .set_parent(&self.window.window)
                .set_file_name(suggested.file_name().unwrap_or_default().to_string_lossy());
            if let Some(parent) = suggested
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
            {
                dialog = dialog.set_directory(parent);
            }
            self.canvas.save_dialog_result(dialog.save_file());
            self.cancel_input();
            self.frame_rate.reset();
            self.request_repaint();
        }
        Ok(())
    }

    pub(super) fn request_repaint(&mut self) {
        self.frames.request_repaint(Instant::now());
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.pen.detach();
    }
}
