//! One session's transcript: its history by range and its live output, on the kit's `MessageScroller`
//! and `TextView`. `docs/ui.md` has why it's built on those. Everything GPUI about a transcript is in
//! here, and the data is in `client_core::session_log`, which knows nothing about rendering.

#![warn(clippy::wildcard_enum_match_arm)]

pub mod look;
mod log_block;
mod questions;
mod rows;
mod transcript;

pub use transcript::{Transcript, TranscriptEvent, mode_label};
