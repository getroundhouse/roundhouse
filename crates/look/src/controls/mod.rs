//! Buttons: every kind of button, and where each goes. A button's kind is what it's for, and its size is one of a
//! control's, chosen by where it sits, so the buttons in the same kind of place are the same size, wherever that is.
//! `docs/spec/components.md` has the table of its kinds, which a test holds to these.
//!
//! Each size leaves a little more room around a button's words and icon than the kit's own sizes do.

#[cfg(test)]
mod tests;

mod button;
mod kinds;
mod sizing;
pub use button::{Button, MenuButton, Side, button, glyph_button, icon_button};
pub use kinds::ButtonKind;
pub use sizing::Sizing;
