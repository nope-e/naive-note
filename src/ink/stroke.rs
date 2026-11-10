//! Stroke lifetime, immutable confirmed records and a disposable prediction layer.

use std::sync::Arc;

use tablet_input_windows::{PenPhase as NativePhase, PenSample};
use uuid::Uuid;

use crate::document::{
    ModeledSample, NativePenData, PathCommand, PenPhase, PenStyle, Point, ProcessingSnapshot,
    RawSample, StrokeRecord,
};

use super::{
    modeler::InkModeler,
    outline::{OutlineState, append_outline},
    target::StrokeTarget,
};

/// Predictions deliberately have no serialization or document-storage path.
#[derive(Clone)]
pub struct ActiveInk {
    pub record: Arc<StrokeRecord>,
    pub predicted_outline: Arc<[PathCommand]>,
}

pub struct LiveStroke {
    target: StrokeTarget,
    record: Arc<StrokeRecord>,
    modeler: Option<InkModeler>,
    predicted_outline: Arc<[PathCommand]>,
    outline_state: OutlineState,
}

impl LiveStroke {
    pub fn new(target: StrokeTarget, smoothing: bool, style: PenStyle) -> Self {
        let modeler = smoothing.then(InkModeler::new);
        let processing = modeler
            .as_ref()
            .map_or_else(direct_processing, InkModeler::processing);
        Self {
            target,
            modeler,
            predicted_outline: Arc::from([]),
            outline_state: OutlineState::default(),
            record: Arc::new(StrokeRecord {
                id: Uuid::new_v4(),
                page_index: target.page_index,
                raw_samples: Vec::new(),
                modeled_samples: Vec::new(),
                outline: Vec::new(),
                style,
                processing,
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
        // and timestamps remain intact in raw data in both processing modes.
        if !raw.position.is_finite() || raw.pressure.is_some_and(|p| !p.is_finite()) {
            return;
        }
        Arc::make_mut(&mut self.record)
            .raw_samples
            .push(raw.clone());
        if let Some(modeler) = self.modeler.as_mut() {
            match modeler.push(&raw) {
                Ok(samples) => {
                    append_samples(
                        Arc::make_mut(&mut self.record),
                        &mut self.outline_state,
                        samples,
                    );
                    self.refresh_prediction();
                }
                Err(error) => self.fallback(&error),
            }
        } else {
            append_samples(
                Arc::make_mut(&mut self.record),
                &mut self.outline_state,
                [direct_sample(&raw)],
            );
        }
    }

    fn refresh_prediction(&mut self) {
        self.predicted_outline = Arc::from([]);
        let Some(modeler) = self.modeler.as_mut() else {
            return;
        };
        match modeler.predict() {
            Ok(samples) => {
                let mut path = Vec::new();
                let mut previous = self.outline_state.anchor();
                for sample in &samples {
                    append_outline(&mut path, previous, sample, &self.record.style);
                    previous = Some(sample);
                }
                self.predicted_outline = path.into();
            }
            Err(error) => self.fallback(&error),
        }
    }

    fn fallback(&mut self, error: &str) {
        log::warn!("Stroke modeler failed; rebuilding complete stroke as Direct: {error}");
        self.modeler = None;
        self.predicted_outline = Arc::from([]);
        let record = Arc::make_mut(&mut self.record);
        record.processing = direct_processing();
        record.modeled_samples = record.raw_samples.iter().map(direct_sample).collect();
        record.outline.clear();
        self.outline_state = OutlineState::default();
        for sample in &record.modeled_samples {
            self.outline_state
                .append(&mut record.outline, sample, &record.style, false);
        }
    }

    pub fn snapshot(&self) -> ActiveInk {
        ActiveInk {
            record: Arc::clone(&self.record),
            predicted_outline: Arc::clone(&self.predicted_outline),
        }
    }

    pub fn finish(mut self) -> Option<Arc<StrokeRecord>> {
        // Ctrl/Alt tool switches and releases without a position also finalize
        // at the last ink sample. This synthetic Up never changes the raw log.
        if let Some(modeler) = self.modeler.as_mut() {
            match modeler.finish() {
                Ok(samples) => append_samples(
                    Arc::make_mut(&mut self.record),
                    &mut self.outline_state,
                    samples,
                ),
                Err(error) => self.fallback(&error),
            }
        }
        let record = Arc::make_mut(&mut self.record);
        if record.style.kind == crate::document::InkKind::Highlighter
            && let Some(last) = record.modeled_samples.last()
        {
            // Coalescing may skip a tiny last Move, but not the final endpoint.
            self.outline_state
                .append(&mut record.outline, last, &record.style, true);
        }
        (!self.record.raw_samples.is_empty()).then_some(self.record)
    }
}

fn direct_processing() -> ProcessingSnapshot {
    ProcessingSnapshot {
        algorithm: "direct-round-outline".to_owned(),
        version: 1,
        coordinate_unit: "paper-pt-top-left".to_owned(),
        modeler: None,
    }
}

fn direct_sample(raw: &RawSample) -> ModeledSample {
    ModeledSample {
        position: raw.position,
        time_us: raw.time_us,
        pressure: raw.pressure.unwrap_or(0.5).clamp(0.0, 1.0),
    }
}

fn append_samples(
    record: &mut StrokeRecord,
    state: &mut OutlineState,
    samples: impl IntoIterator<Item = ModeledSample>,
) {
    for sample in samples {
        state.append(&mut record.outline, &sample, &record.style, false);
        record.modeled_samples.push(sample);
    }
}

#[cfg(test)]
mod tests;
