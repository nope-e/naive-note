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
