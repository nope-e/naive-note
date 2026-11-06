//! Renderer-independent page geometry, viewport transforms and stroke records.

mod geometry;
mod stroke;
mod view;

pub use geometry::{PageGeometry, Point, Rect, Rotation, Size};
pub use stroke::{
    ModeledSample, PathCommand, PenPhase, PenStyle, ProcessingSnapshot, RawSample, StrokeRecord,
};
pub use view::{CanvasViewport, ViewTransform};
