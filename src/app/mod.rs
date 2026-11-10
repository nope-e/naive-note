//! Event-loop startup and shared application state.

mod canvas;
mod events;
mod frame_rate;
mod frame_scheduler;
mod runtime;
pub(crate) mod theme;
mod window_frame;

use std::{path::PathBuf, time::Instant};

use anyhow::{Context, Result};
use winit::event_loop::{EventLoop, EventLoopProxy};

use crate::platform::PenInput;

use runtime::Runtime;

pub enum UserEvent {
    Repaint(Instant),
    PdfResponse(Box<crate::pdf::PdfResponse>),
    StorageResponse(Box<crate::storage::preferences::StorageResponse>),
}

pub fn run(startup_pdf: Option<PathBuf>) -> Result<()> {
    let pen = PenInput::default();
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    pen.install(&mut builder);
    let event_loop = builder.build()?;
    let mut app = Application {
        runtime: None,
        startup_pdf,
        proxy: event_loop.create_proxy(),
        error: None,
        show_renderer_info: false,
        pen,
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
    startup_pdf: Option<PathBuf>,
    proxy: EventLoopProxy<UserEvent>,
    error: Option<anyhow::Error>,
    show_renderer_info: bool,
    pen: PenInput,
}
