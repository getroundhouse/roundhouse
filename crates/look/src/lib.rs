//! How the app looks, everywhere it's drawn: the theme over the kit's, the few colours the kit has no name for,
//! code's colours, the status mark, and the app icon. `docs/spec/look.md` has the rules these follow.

#![warn(clippy::wildcard_enum_match_arm)]

mod clock;
mod fan;
mod highlight;
mod icon;
mod mark;
mod palette;
mod theme;
mod work;

#[cfg(feature = "testing")]
pub use clock::hold_still;
pub use highlight::{Spans, highlight, preload_highlighting, try_highlight};
pub use icon::{IconSize, IconStyle, icon};
pub use mark::{Signal, Size, brand, mark};
pub use palette::{faint, tab_underline};
pub use theme::apply;
pub use work::Work;
