//! Every corner the app rounds, named by what it's on rather than by its size, so a corner means the same wherever it's
//! drawn, and two that are the same size today can change apart, and the widths of an edge and of the lines that
//! stand for one. `docs/spec/look.md` has the table of the corners, which a test holds to these.
//!
//! They're in pixels rather than rems, as a card's and a control's were in the kit's theme, so a corner stays as round
//! whatever size text is set at.

#[cfg(test)]
mod tests;

mod corners;
pub use corners::{Corner, EDGE, QUOTE_RULE, Shaped, TAB_LINE};
