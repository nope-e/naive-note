//! Owned, serializable paper geometry, shared by Skia and PDF appearance output.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Point;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PenStyle {
    pub color_rgb: [u8; 3],
    pub max_width_pt: f64,
    pub min_width_ratio: f64,
    pub pressure_gamma: f64,
}

impl PenStyle {
    pub fn width(&self, pressure: f64) -> f64 {
        self.max_width_pt
            * (self.min_width_ratio
                + (1.0 - self.min_width_ratio) * pressure.clamp(0.0, 1.0).powf(self.pressure_gamma))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PenPhase {
    Down,
    Move,
    Up,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RawSample {
    pub position: Point,
    pub time_us: u64,
    pub pressure: Option<f64>,
    pub tilt_degrees: Option<[f64; 2]>,
    pub rotation_degrees: Option<f64>,
    pub phase: PenPhase,
    /// Original device attributes, including independently missing tilt axes.
    /// Absent in older version-1 attachments and synthetic samples.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<NativePenData>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NativePenData {
    pub pointer_id: u32,
    pub stroke_id: u64,
    pub client_position_px: [f64; 2],
    pub pressure_level: Option<i32>,
    pub tilt_x_degrees: Option<f64>,
    pub tilt_y_degrees: Option<f64>,
    pub eraser: bool,
    pub inverted: bool,
    pub barrel_button: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModeledSample {
    pub position: Point,
    pub time_us: u64,
    pub pressure: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProcessingSnapshot {
    pub algorithm: String,
    pub version: u32,
    pub coordinate_unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modeler: Option<ModelerPreset>,
}

/// Parameters in paper points and seconds, captured once at stroke start.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelerPreset {
    pub wobble_smoother_timeout: f64,
    pub wobble_smoother_speed_floor: f64,
    pub wobble_smoother_speed_ceiling: f64,
    pub position_modeler_spring_mass_constant: f64,
    pub position_modeler_drag_constant: f64,
    pub sampling_min_output_rate: f64,
    pub sampling_end_of_stroke_stopping_distance: f64,
    pub sampling_end_of_stroke_max_iterations: usize,
    pub sampling_max_outputs_per_call: usize,
    pub stylus_state_modeler_max_input_samples: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PathCommand {
    MoveTo(Point),
    LineTo(Point),
    CubicTo {
        control1: Point,
        control2: Point,
        to: Point,
    },
    Close,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrokeRecord {
    pub id: Uuid,
    pub page_index: u32,
    pub raw_samples: Vec<RawSample>,
    pub modeled_samples: Vec<ModeledSample>,
    pub outline: Vec<PathCommand>,
    pub style: PenStyle,
    pub processing: ProcessingSnapshot,
}

#[cfg(test)]
mod tests;
