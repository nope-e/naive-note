//! Incremental direct strokes with raw samples and shared immutable snapshots.

use std::sync::Arc;

use tablet_input_windows::{PenPhase as NativePhase, PenSample};
use uuid::Uuid;

use crate::document::{
    ModeledSample, NativePenData, PenPhase, PenStyle, Point, ProcessingSnapshot, RawSample,
    StrokeRecord,
};

use super::{outline::append_outline, target::StrokeTarget};

pub struct DirectStroke {
    target: StrokeTarget,
    record: Arc<StrokeRecord>,
}

impl DirectStroke {
    pub fn new(target: StrokeTarget) -> Self {
        Self {
            target,
            record: Arc::new(StrokeRecord {
                id: Uuid::new_v4(),
                page_index: target.page_index,
                raw_samples: Vec::new(),
                modeled_samples: Vec::new(),
                outline: Vec::new(),
                style: PenStyle {
                    color_rgb: [25, 105, 190],
                    max_width_pt: 2.0,
                    min_width_ratio: 0.2,
                    pressure_gamma: 1.5,
                },
                processing: ProcessingSnapshot {
                    algorithm: "direct-round-outline".to_owned(),
                    version: 1,
                    coordinate_unit: "paper-pt-top-left".to_owned(),
                },
            }),
        }
    }

    pub fn push_pen(&mut self, sample: PenSample) {
        let phase = match sample.phase {
            NativePhase::Down => PenPhase::Down,
            NativePhase::Move => PenPhase::Move,
            NativePhase::Up => PenPhase::Up,
            _ => return,
        };
        self.push(RawSample {
            position: self
                .target
                .paper_position(Point::new(sample.position[0], sample.position[1])),
            time_us: sample.time_us,
            pressure: sample.pressure.map(f64::from),
            tilt_degrees: sample
                .tilt_deg
                .x
                .zip(sample.tilt_deg.y)
                .map(|(x, y)| [f64::from(x), f64::from(y)]),
            rotation_degrees: sample.rotation_deg.map(f64::from),
            phase,
            native: Some(NativePenData {
                pointer_id: sample.pointer_id,
                stroke_id: sample.stroke_id.expect("contact identifier"),
                client_position_px: sample.position,
                pressure_level: sample.pressure_level,
                tilt_x_degrees: sample.tilt_deg.x.map(f64::from),
                tilt_y_degrees: sample.tilt_deg.y.map(f64::from),
                eraser: sample.eraser,
                inverted: sample.inverted,
                barrel_button: sample.barrel_button,
            }),
        });
    }

    pub fn push_mouse(&mut self, physical: Point, time_us: u64, phase: PenPhase) {
        self.push(RawSample {
            position: self.target.paper_position(physical),
            time_us,
            pressure: Some(0.5),
            tilt_degrees: None,
            rotation_degrees: None,
            phase,
            native: None,
        });
    }

    fn push(&mut self, raw: RawSample) {
        // Do not allow malformed input to poison a Skia path. Repeated positions
        // and timestamps remain in the raw data; Direct needs no time resampling.
        if !raw.position.is_finite() || raw.pressure.is_some_and(|p| !p.is_finite()) {
            return;
        }
        let record = Arc::make_mut(&mut self.record);
        let modeled = ModeledSample {
            position: raw.position,
            time_us: raw.time_us,
            pressure: raw.pressure.unwrap_or(0.5).clamp(0.0, 1.0),
        };
        append_outline(
            &mut record.outline,
            record.modeled_samples.last(),
            &modeled,
            &record.style,
        );
        record.raw_samples.push(raw);
        record.modeled_samples.push(modeled);
    }

    pub fn snapshot(&self) -> Arc<StrokeRecord> {
        Arc::clone(&self.record)
    }

    pub fn finish(self) -> Option<Arc<StrokeRecord>> {
        (!self.record.raw_samples.is_empty()).then_some(self.record)
    }
}
