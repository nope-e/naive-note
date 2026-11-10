//! Runtime-selectable stroke modeling, pressure outlines and input mapping.

mod modeler;
mod outline;
mod stroke;
mod target;

use outline::round_outline;
pub use stroke::{ActiveInk, LiveStroke};
pub use target::StrokeTarget;

#[cfg(test)]
pub(crate) mod test_support;
