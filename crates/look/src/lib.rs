//! How the app looks, everywhere it's drawn: the theme over the kit's, the few colours the kit has no name for,
//! code's colours, the status mark, the ring for a background task, the app icon, the pointing hand, how long things
//! take to move, a hover card that waits like a tooltip, how a count of tokens reads, and the time text on screen
//! reads, which pictures hold still with everything that moves. `docs/spec/look.md` has the rules these follow.

#![warn(clippy::wildcard_enum_match_arm)]

mod clock;
mod fan;
mod hand;
mod highlight;
mod hover;
mod icon;
mod mark;
pub mod motion;
mod palette;
mod raised;
mod task_ring;
mod theme;
mod tokens;
mod work;

#[cfg(feature = "testing")]
pub use clock::hold_still;
pub use clock::{date, date_and_time, day_and_time, local_time, now_ms, time_of_day};
pub use hand::{Hand, Off, hand_once_laid_out};
pub use highlight::{Spans, highlight, language, preload_highlighting, try_highlight};
pub use hover::hover_card;
pub use icon::{IconSize, IconStyle, icon};
pub use mark::{Signal, Size, brand, mark};
pub use palette::{faint, field_edge, prompt, tab_underline};
pub use raised::raised;
pub use task_ring::{TaskRing, task_ring};
pub use theme::apply;
pub use tokens::tokens;
pub use work::Work;
