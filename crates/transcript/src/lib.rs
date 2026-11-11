//! One session's transcript: its history by range and its live output, in a list of its own and the kit's
//! `TextView`. `docs/ui.md` has why it's built on those. Everything GPUI about a transcript is in
//! here, and the data is in `client_core::session_log`, which knows nothing about rendering.

#![warn(clippy::wildcard_enum_match_arm)]

mod attachments;
mod calls;
#[cfg(test)]
mod fixtures;
mod folds;
mod images_apart;
mod item_key;
mod linked_images;
mod list;
mod log_block;
mod long_texts;
mod questions;
mod rows;
mod scroll_area;
mod selectable_lines;
mod transcript;

pub use attachments::{file_card, image_format};
pub use calls::agent::shown_kind;
pub use list::{Place, TranscriptList};
pub use questions::QuestionPanel;
pub use rows::{around_column, in_column, subagent_standing};
pub use transcript::{Transcript, TranscriptEvent};

/// How long a new question ignores keys, which a test can wait out.
#[cfg(feature = "testing")]
pub const QUESTION_SETTLES: std::time::Duration = questions::SETTLE;
