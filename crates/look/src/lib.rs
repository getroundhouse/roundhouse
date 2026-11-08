//! How the app looks, everywhere it's drawn: the theme over the kit's, the few colours the kit has no name for,
//! and the status mark. `docs/spec/look.md` has the rules these follow.

#![warn(clippy::wildcard_enum_match_arm)]

mod clock;
mod mark;
mod palette;
mod theme;

#[cfg(feature = "testing")]
pub use clock::hold_still;
pub use mark::{Signal, Size, Work, brand, mark};
pub use palette::faint;
pub use theme::apply;
