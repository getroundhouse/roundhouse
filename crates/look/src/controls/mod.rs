//! Every size a button comes in, and where each goes. They're sizes, ordered from the smallest, and each is chosen by
//! where a button sits, so the buttons in the same kind of place are the same size, wherever that is.
//! `docs/spec/look.md` has the table of them, which a test holds to these.
//!
//! Each leaves a little more room around a button's words and icon than the kit's own sizes do.

#[cfg(test)]
mod tests;

mod sizing;
pub use sizing::{ControlSize, Sizing};
