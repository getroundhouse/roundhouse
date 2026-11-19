//! Empty states. A pane with nothing in it says what's missing, or what it can't show, and why, with what to do about
//! it, under the mark, and a list or a pane with nothing to show that needs nothing done says so in one muted line.
//! `docs/spec/components.md` has the table of an empty pane's kinds, which a test holds to these.

#[cfg(test)]
mod tests;

mod empty;
mod kinds;
mod nothing;
pub use empty::{Empty, empty};
pub use kinds::EmptyKind;
pub use nothing::{NothingToShow, nothing_to_show};
