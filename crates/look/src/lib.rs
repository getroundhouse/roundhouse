//! How the app looks, everywhere it's drawn: the theme over the kit's, the few colours the kit has no name for, code's
//! colours, the words that changed within a diff's lines, text of different sizes on one baseline, the status mark, the
//! ring for a background task and the robot for a subagent at work there, with the size of a call's icon, which the
//! robot shares, how an image is fitted to the room there is, the app icon, plan mode's texture, which is made from the
//! mark, the pointing hand, the button that copies, the parts of the window tests find by where they're drawn, how long
//! things take to move, a hover card that waits like a tooltip, a chat's cache, how a count of tokens reads, and the
//! time text on screen reads, which pictures hold still with everything that moves. `docs/spec/look.md` has the rules
//! these follow.

#![warn(clippy::wildcard_enum_match_arm)]

mod baseline;
mod cache;
mod clock;
mod copy;
mod fan;
mod fit;
mod hand;
mod highlight;
mod hover;
mod icon;
mod mark;
pub mod motion;
mod palette;
mod plan;
mod raised;
mod region;
mod robot;
mod task_ring;
mod theme;
mod tokens;
mod word_changes;
mod work;

pub use baseline::{LineText, Placed};
pub use cache::{CacheMark, cache_mark, expires_in, minutes_left, next_minute_in_ms};
#[cfg(feature = "testing")]
pub use clock::hold_still;
pub use clock::{date, date_and_time, day_and_time, local_time, now_ms, time_of_day};
pub use copy::copy_button;
pub use fit::{IMAGE_EDGE, fit, pixels};
pub use hand::{Hand, Off};
pub use highlight::{Spans, highlight, language, preload_highlighting, try_highlight};
pub use hover::{hover_card, hover_card_with_a_button};
pub use icon::{IconSize, IconStyle, icon};
pub use mark::{Signal, Size, brand, chat_mark, mark};
pub use palette::{faint, field_edge, found, found_here, prompt, tab_underline};
pub use plan::{plan_colors, plan_texture};
pub use raised::raised;
pub use region::{InRegion, Region};
pub use robot::{LINE_ICON, robot_at_work};
pub use task_ring::{TaskRing, task_ring};
pub use theme::apply;
pub use tokens::tokens;
pub use word_changes::{LineChange, word_changes};
pub use work::Work;
