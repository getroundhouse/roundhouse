//! The words a dictation is biased toward, and what they were picked from, so one that's kept can be heard again with
//! the words another way of picking them would give. A kept recording's file has these as they are, so a change to one
//! changes its form, as `kept_recordings::VERSION` says.

#![warn(clippy::wildcard_enum_match_arm)]

mod picked_from;
mod words;

pub use picked_from::{Asked, Choice, PickedFrom, Places, Proposal, Workspace};
pub use words::{HintWord, Picked};
