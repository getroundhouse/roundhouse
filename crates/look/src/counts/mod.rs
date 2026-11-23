//! Counts: how many, as a number in the text, never in a box, and the lines added and removed, in their colours, each
//! written whole unless it's asked to be shortened. `docs/spec/components.md` has the tables of a count's kinds and of
//! the lines' kinds, which tests hold to these.

#[cfg(test)]
mod tests;

mod count;
mod kinds;
mod lines;
pub use count::{Count, count};
pub use kinds::{CountKind, LinesKind};
pub use lines::{LinesChanged, lines_changed};
