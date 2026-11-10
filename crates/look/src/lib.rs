//! How the app looks, everywhere it's drawn: the theme over the kit's, the few colours the kit has no name for,
//! code's colours, the status mark, the app icon, and the time text on screen reads, which pictures hold still with
//! the marks. `docs/spec/look.md` has the rules these follow.

#![warn(clippy::wildcard_enum_match_arm)]

mod clock;
mod fan;
mod highlight;
mod icon;
mod mark;
mod palette;
mod raised;
mod theme;
mod work;

#[cfg(feature = "testing")]
pub use clock::hold_still;
pub use clock::{day_and_time, local_time, now_ms};
pub use highlight::{Spans, highlight, preload_highlighting, try_highlight};
pub use icon::{IconSize, IconStyle, icon};
pub use mark::{Signal, Size, brand, mark};
pub use palette::{faint, prompt, tab_underline};
pub use raised::raised;
pub use theme::apply;
pub use work::Work;
