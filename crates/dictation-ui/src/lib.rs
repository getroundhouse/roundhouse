//! Dictation attached to an existing text field. The app supplies recordings from its model and keeps its settings;
//! each attachment owns its recording, tentative words and cancellation snapshot. Final words emit the input's usual
//! change event, so the host saves them as typing. Hosts enable only visible targets, draw the optional live line and
//! microphone, and retain their own submission actions. Nothing here knows a hub, chat or question.

#![warn(clippy::wildcard_enum_match_arm)]

mod controls;
mod field;
mod fine_line;
mod holding;
mod keys;
mod recording;
mod source;
mod text;
mod view;

pub use controls::{CANCEL, STOP};
pub use dictation::Hints;
pub use field::{Event, Field};
pub use holding::{SPACE_DICTATES_AFTER, SPACE_LISTENS_AFTER};
pub use keys::{CancelDictation, HOLD_TO_DICTATE_KEY, HoldToDictate, bind_keys, window_left};
pub use recording::NOTHING_HEARD;
pub use source::{Source, install, unavailable};
pub use view::LIVE_LINE;
