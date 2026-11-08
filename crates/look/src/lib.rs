//! How the app looks, everywhere it's drawn: the theme over the kit's, the few colours the kit has no name for,
//! and the status mark. `docs/spec/look.md` has the rules these follow.

#![warn(clippy::wildcard_enum_match_arm)]

mod mark;
mod palette;
mod theme;

pub use mark::{Signal, Work, brand, mark};
pub use palette::{accent, faint, raised};
pub use theme::apply;
