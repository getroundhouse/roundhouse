//! Counts: how many, as a number in the text, never in a box, and the lines added to and removed from a file, in their
//! colours. `docs/spec/components.md` has the table of a count's kinds, which a test holds to these.

#[cfg(test)]
mod tests;

mod count;
mod kinds;
mod lines;
pub use count::{Count, count};
pub use kinds::CountKind;
pub use lines::{LinesChanged, lines_changed};
