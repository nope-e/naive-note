//! Average presentation rate over a short sampling interval.

use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct FrameRate {
    started_at: Option<Instant>,
    frames: u32,
    fps: Option<f64>,
}

impl FrameRate {
    pub fn begin_frame(&mut self) {
        self.started_at.get_or_insert_with(Instant::now);
    }

    pub fn frame_presented(&mut self) {
        let Some(started_at) = self.started_at else {
            return;
        };
        self.frames += 1;
        let now = Instant::now();
        let elapsed = now.duration_since(started_at);
        if elapsed >= Duration::from_millis(500) {
            self.fps = Some(f64::from(self.frames) / elapsed.as_secs_f64());
            self.started_at = Some(now);
            self.frames = 0;
        }
    }

    pub fn fps(&self) -> Option<f64> {
        self.fps
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
