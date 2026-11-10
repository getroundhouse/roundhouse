//! One session's transcript: its history by range and its live output, on GPUI's virtual list and the kit's
//! `TextView`. `docs/ui.md` has why it's built on those. Everything GPUI about a transcript is in
//! here, and the data is in `client_core::session_log`, which knows nothing about rendering.

#![warn(clippy::wildcard_enum_match_arm)]

mod attachments;
mod calls;
#[cfg(test)]
mod fixtures;
mod folds;
mod item_key;
mod linked_images;
mod log_block;
mod long_texts;
mod questions;
mod rows;
mod scroll_area;
mod scroller;
mod sketch;
mod transcript;

pub use attachments::{file_card, image_format};
pub use rows::{FocusStatus, FocusSummary, around_column, in_column};
pub use scroller::Scroller;
pub use sketch::{Sketch, set_sketch, sketch};
pub use transcript::{Transcript, TranscriptEvent, mode_label};
