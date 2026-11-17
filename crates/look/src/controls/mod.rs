//! Buttons: every kind of button and every size it comes in, and where each goes. A button's kind is what it's for,
//! and its size is chosen by where it sits, so the buttons in the same kind of place are the same size, wherever that
//! is. `docs/spec/components.md` has the tables of both, which tests hold to these.
//!
//! Each size leaves a little more room around a button's words and icon than the kit's own sizes do.

#[cfg(test)]
mod tests;

mod button;
mod kinds;
mod sizing;
pub use button::{Button, Menu, Side, button, glyph_button, icon_button};
pub use kinds::ButtonKind;
pub use sizing::{ControlSize, Sizing};
