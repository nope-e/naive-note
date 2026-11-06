//! Demand-driven frame deadlines. This module never sleeps or touches a window.

use std::time::{Duration, Instant};

pub(super) const MAX_FPS: u32 = 60;
const FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000_u64.div_ceil(MAX_FPS as u64));

#[derive(Default)]
pub(super) struct FrameScheduler {
    last_frame: Option<Instant>,
    repaint_at: Option<Instant>,
}

impl FrameScheduler {
    pub fn request_repaint(&mut self, at: Instant) {
        self.repaint_at = Some(self.repaint_at.map_or(at, |pending| pending.min(at)));
    }

    pub fn next_repaint(&self) -> Option<Instant> {
        self.repaint_at.map(|at| match self.last_frame {
            Some(last) => at.max(last + FRAME_INTERVAL),
            None => at,
        })
    }

    /// Also gate OS-generated redraws, which can arrive before our deadline.
    pub fn begin_frame(&mut self, now: Instant) -> bool {
        if self
            .last_frame
            .is_some_and(|last| now < last + FRAME_INTERVAL)
        {
            self.request_repaint(now);
            return false;
        }
        // Measure starts, not swap completion: vsync/render time already belongs
        // to this interval. A slow frame must not cause a burst of catch-up frames.
        self.last_frame = Some(now);
        if self.repaint_at.is_some_and(|at| at <= now) {
            self.repaint_at = None;
        }
        true
    }

    pub fn cancel_repaint(&mut self) {
        self.repaint_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_has_no_deadline_and_completed_frame_does_not_rearm_itself() {
        let mut scheduler = FrameScheduler::default();
        let now = Instant::now();
        assert_eq!(scheduler.next_repaint(), None);
        scheduler.request_repaint(now);
        assert_eq!(scheduler.next_repaint(), Some(now));
        assert!(scheduler.begin_frame(now));
        assert_eq!(scheduler.next_repaint(), None);
    }

    #[test]
    fn redraw_storm_is_capped_without_losing_the_pending_frame() {
        let mut scheduler = FrameScheduler::default();
        let start = Instant::now();
        assert!(scheduler.begin_frame(start));
        for milliseconds in 1..=16 {
            let now = start + Duration::from_millis(milliseconds);
            scheduler.request_repaint(now);
            assert!(!scheduler.begin_frame(now));
            assert_eq!(scheduler.next_repaint(), Some(start + FRAME_INTERVAL));
        }
        assert!(scheduler.begin_frame(start + FRAME_INTERVAL));
        assert_eq!(scheduler.next_repaint(), None);
        assert!(FRAME_INTERVAL.as_secs_f64() >= 1.0 / f64::from(MAX_FPS));
    }

    #[test]
    fn earlier_input_advances_an_egui_timer_but_respects_the_cap() {
        let mut scheduler = FrameScheduler::default();
        let start = Instant::now();
        assert!(scheduler.begin_frame(start));
        scheduler.request_repaint(start + Duration::from_secs(1));
        scheduler.request_repaint(start + Duration::from_millis(2));
        scheduler.request_repaint(start + Duration::from_secs(2));
        assert_eq!(scheduler.next_repaint(), Some(start + FRAME_INTERVAL));
    }

    #[test]
    fn unrelated_frame_preserves_a_future_repaint() {
        let mut scheduler = FrameScheduler::default();
        let start = Instant::now();
        let future = start + Duration::from_secs(1);
        scheduler.request_repaint(future);
        assert!(scheduler.begin_frame(start));
        assert_eq!(scheduler.next_repaint(), Some(future));
        assert!(scheduler.begin_frame(future));
        assert_eq!(scheduler.next_repaint(), None);
    }

    #[test]
    fn slow_frame_does_not_add_another_interval_or_allow_catch_up() {
        let mut scheduler = FrameScheduler::default();
        let start = Instant::now();
        assert!(scheduler.begin_frame(start));
        let finished = start + Duration::from_millis(40);
        scheduler.request_repaint(finished);
        assert_eq!(scheduler.next_repaint(), Some(finished));
        assert!(scheduler.begin_frame(finished));
        assert!(!scheduler.begin_frame(finished + Duration::from_millis(1)));
        assert_eq!(scheduler.next_repaint(), Some(finished + FRAME_INTERVAL));
    }

    #[test]
    fn hiding_cancels_work_and_restore_still_obeys_the_frame_interval() {
        let mut scheduler = FrameScheduler::default();
        let start = Instant::now();
        assert!(scheduler.begin_frame(start));
        scheduler.request_repaint(start);
        scheduler.cancel_repaint();
        assert_eq!(scheduler.next_repaint(), None);
        scheduler.request_repaint(start + Duration::from_millis(1));
        assert_eq!(scheduler.next_repaint(), Some(start + FRAME_INTERVAL));
        assert!(scheduler.begin_frame(start + Duration::from_secs(5)));
        assert_eq!(scheduler.next_repaint(), None);
    }
}
