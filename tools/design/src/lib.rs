//! Reads the design, the Brilliant project in `design/`, as `docs/standards/design.md` says it's laid out, and writes
//! its index from each canvas's status badge.

mod error;
mod files;
mod index;
mod status;

pub use error::Error;
pub use files::{CanvasPath, File, canvases, films};
pub use index::{INDEX, Level, index, listed};
pub use status::Status;
