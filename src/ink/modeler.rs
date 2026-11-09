//! Adapter for the pinned modeler. Raw timestamps/attributes are never rewritten.

use ink_stroke_modeler_rs::{
    ModelerInput, ModelerInputEventType as Event, ModelerParams, ModelerResult, StrokeModeler,
};

use crate::document::{
    ModeledSample, ModelerPreset, PenPhase, Point, ProcessingSnapshot, RawSample,
};

pub(super) struct InkModeler {
    engine: StrokeModeler,
    origin_us: Option<u64>,
    latest: Option<ModelerInput>,
    latest_raw: Option<RawSample>,
    ended: bool,
}

impl InkModeler {
    pub fn new() -> Self {
        Self {
            engine: StrokeModeler::new(params()).expect("valid application modeler preset"),
            origin_us: None,
            latest: None,
            latest_raw: None,
            ended: false,
        }
    }

    pub fn processing(&self) -> ProcessingSnapshot {
        let p = params();
        ProcessingSnapshot {
            algorithm: "ink-stroke-modeler-rs@0.1.0-round-outline".to_owned(),
            version: 1,
            coordinate_unit: "paper-pt-top-left".to_owned(),
            modeler: Some(ModelerPreset {
                wobble_smoother_timeout: p.wobble_smoother_timeout,
                wobble_smoother_speed_floor: p.wobble_smoother_speed_floor,
                wobble_smoother_speed_ceiling: p.wobble_smoother_speed_ceiling,
                position_modeler_spring_mass_constant: p.position_modeler_spring_mass_constant,
                position_modeler_drag_constant: p.position_modeler_drag_constant,
                sampling_min_output_rate: p.sampling_min_output_rate,
                sampling_end_of_stroke_stopping_distance: p
                    .sampling_end_of_stroke_stopping_distance,
                sampling_end_of_stroke_max_iterations: p.sampling_end_of_stroke_max_iterations,
                sampling_max_outputs_per_call: p.sampling_max_outputs_per_call,
                stylus_state_modeler_max_input_samples: p.stylus_state_modeler_max_input_samples,
            }),
        }
    }

    pub fn push(&mut self, raw: &RawSample) -> Result<Vec<ModeledSample>, String> {
        if self
            .latest_raw
            .as_ref()
            .is_some_and(|last| raw.time_us < last.time_us)
        {
            return Err("raw timestamp moved backwards".to_owned());
        }
        let origin = *self.origin_us.get_or_insert(raw.time_us);
        let mut input = ModelerInput {
            event_type: match raw.phase {
                PenPhase::Down => Event::Down,
                PenPhase::Move => Event::Move,
                PenPhase::Up => Event::Up,
            },
            pos: (raw.position.x, raw.position.y),
            time: (raw.time_us - origin) as f64 / 1_000_000.0,
            pressure: raw.pressure.unwrap_or(0.5).clamp(0.0, 1.0),
        };
        self.latest_raw = Some(raw.clone());
        if let Some(last) = &self.latest
            && input.time <= last.time
        {
            // Coalesce same-time moves at the adapter boundary. Keep their
            // latest endpoint for finish; the complete originals stay in raw.
            if raw.phase == PenPhase::Move {
                return Ok(Vec::new());
            }
            // Up must reach the engine even for a zero-duration tap.
            input.time = last.time + 0.000_001;
        }
        self.update(input)
    }

    fn update(&mut self, input: ModelerInput) -> Result<Vec<ModeledSample>, String> {
        let results = self
            .engine
            .update(input.clone())
            .map_err(|e| e.to_string())?;
        self.ended = input.event_type == Event::Up;
        self.latest = Some(input);
        self.convert(results)
    }

    pub fn predict(&mut self) -> Result<Vec<ModeledSample>, String> {
        if self.ended || self.latest.is_none() {
            return Ok(Vec::new());
        }
        let results = self.engine.predict()?;
        self.convert(results)
    }

    pub fn finish(&mut self) -> Result<Vec<ModeledSample>, String> {
        if self.ended {
            return Ok(Vec::new());
        }
        let Some(mut last) = self.latest_raw.clone() else {
            return Ok(Vec::new());
        };
        last.phase = PenPhase::Up;
        self.push(&last)
    }

    fn convert(&self, results: Vec<ModelerResult>) -> Result<Vec<ModeledSample>, String> {
        if results.len() > params().sampling_max_outputs_per_call {
            return Err("modeler exceeded output limit".to_owned());
        }
        results
            .into_iter()
            .map(|result| {
                let position = Point::new(result.pos.0, result.pos.1);
                if !position.is_finite()
                    || !result.time.is_finite()
                    || result.time < 0.0
                    || !result.pressure.is_finite()
                {
                    return Err("modeler produced a non-finite sample".to_owned());
                }
                Ok(ModeledSample {
                    position,
                    time_us: self
                        .origin_us
                        .unwrap_or(0)
                        .saturating_add((result.time * 1_000_000.0).round() as u64),
                    pressure: result.pressure.clamp(0.0, 1.0),
                })
            })
            .collect()
    }
}

fn params() -> ModelerParams {
    ModelerParams {
        sampling_min_output_rate: 180.0,
        sampling_max_outputs_per_call: 200,
        ..ModelerParams::suggested()
    }
}
