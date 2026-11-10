//! Renderer-independent page geometry, viewport transforms and stroke records.

mod edit;
mod geometry;
mod layout;
mod stroke;
mod view;

pub use edit::{DocumentRevision, EditCommand, EditHistory};
pub use geometry::{PageGeometry, Point, Rect, Rotation, Size};
pub use layout::PageLayout;
pub use stroke::{
    ModeledSample, ModelerPreset, NativePenData, PathCommand, PenPhase, PenStyle,
    ProcessingSnapshot, RawSample, StrokeRecord,
};
pub use view::{CanvasViewport, ViewTransform};
