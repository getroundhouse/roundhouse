//! Dictation attached to an existing text field. The app supplies recordings from its model and keeps its settings;
//! each attachment owns its recording, tentative words and cancellation snapshot. Final words emit the input's usual
//! change event, so the host saves them as typing. Hosts attach it with [`Field::attach`], enable only visible
//! targets, draw the optional live line and microphone, and keep their own submission actions, which [`Pending`] holds
//! until the last words are in for a host with more than one. Nothing here knows a hub, chat or question. It's a crate
//! of its own because the desktop app and `transcript` both attach it, and `transcript` can't depend on the app.

#![warn(clippy::wildcard_enum_match_arm)]

mod controls;
mod field;
mod fine_line;
mod holding;
mod keyboard;
mod keys;
mod pending;
mod recording;
mod source;
mod text;
mod view;

pub use controls::{CANCEL, STOP};
pub use dictation::Hints;
pub use field::{Event, Field};
pub use holding::{SPACE_DICTATES_AFTER, SPACE_LISTENS_AFTER};
#[cfg(feature = "testing")]
pub use keyboard::{asked_to_see_the_keyboard, hide_the_keyboard, hold_space_down};
pub use keys::{CancelDictation, HOLD_TO_DICTATE_KEY, HoldToDictate, bind_keys, window_activation_changed};
pub use pending::Pending;
pub use recording::NOTHING_HEARD;
pub use source::{Source, install, unavailable};
#[cfg(feature = "testing")]
pub use source::{hear, quiets_while_dictating};
pub use view::LIVE_LINE;
