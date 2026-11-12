//! Runtime-selectable stroke modeling, pressure outlines and input mapping.

mod modeler;
mod outline;
mod stroke;
mod target;

#[cfg(test)]
use outline::round_outline;
pub(crate) use outline::saved_outline_v1;
pub use stroke::{ActiveInk, LiveStroke};
pub use target::StrokeTarget;

#[cfg(test)]
pub(crate) mod test_support;
