//! Owned, serializable paper geometry, shared by Skia and PDF appearance output.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{InkTransform, Point};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InkKind {
    #[default]
    Pen,
    Highlighter,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PenStyle {
    #[serde(default)]
    pub kind: InkKind,
    pub color_rgb: [u8; 3],
    pub max_width_pt: f64,
    pub min_width_ratio: f64,
    pub pressure_gamma: f64,
}

impl Default for PenStyle {
    fn default() -> Self {
        Self {
            kind: InkKind::Pen,
            color_rgb: [25, 105, 190],
            max_width_pt: 2.0,
            min_width_ratio: 0.2,
            pressure_gamma: 1.0,
        }
    }
}

impl PenStyle {
    pub fn highlighter() -> Self {
        Self {
            kind: InkKind::Highlighter,
            color_rgb: [247, 196, 14],
            max_width_pt: 16.0,
            ..Self::default()
        }
    }

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
    /// Informational only: object deletion and rigid transforms never require
    /// verified private capture data. Standard annotations use their current AP.
    #[serde(skip, default = "super::annotation::verified_by_default")]
    pub edit_data_verified: bool,
    #[serde(skip)]
    pub pdf_annotation: Option<std::sync::Arc<super::PdfAnnotationSource>>,
    /// Retained during capture for modeler fallback. Compact edit attachments
    /// omit capture data; object edits use the confirmed geometry below.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_samples: Vec<RawSample>,
    /// Captured raw points are immutable; modeled points and outline are already
    /// in edited paper coordinates. Legacy version-1 records use identity.
    #[serde(default, skip_serializing_if = "InkTransform::is_identity")]
    pub edit_transform: InkTransform,
    pub modeled_samples: Vec<ModeledSample>,
    #[serde(default)]
    pub outline: Vec<PathCommand>,
    pub style: PenStyle,
    pub processing: ProcessingSnapshot,
}

#[cfg(test)]
mod tests;
