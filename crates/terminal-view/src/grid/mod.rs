//! Paints a terminal's frame as a grid of cells, inside one layer, with each cell's character painted as a glyph at its
//! column, as `docs/reference/performance.md` says makes a terminal fast to draw, and says where on it the pointer is
//! pressed and dragged, in cells, which selects.

mod element;
mod glyphs;
mod metrics;
mod painter;
mod pointer;

pub(crate) use element::Grid;
pub(crate) use glyphs::Glyphs;
pub(crate) use metrics::Metrics;
pub(crate) use pointer::Pointing;
