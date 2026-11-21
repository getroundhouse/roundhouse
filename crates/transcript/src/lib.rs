//! One session's transcript: its history by range and its live output, in a list of its own, with its Markdown drawn
//! from the kit's Markdown blocks, and the rest laid out as plain text, with a selection the transcript keeps over its
//! Markdown and most of the plain text its calls show. `docs/decisions/transcript-on-gpui-kit.md` has why it's built
//! on those, and names the records for the list and the texts. Everything GPUI about a transcript is in here, and the
//! data is in `client_core::session_log`, which knows nothing about rendering.

#![warn(clippy::wildcard_enum_match_arm)]

mod attachments;
mod calls;
mod diagrams;
mod document;
mod find;
#[cfg(test)]
mod fixtures;
mod folds;
mod images_apart;
mod images_that_open;
mod item_key;
mod linked_images;
mod list;
mod log_block;
mod model_names;
mod parser_panics;
mod proposal;
mod questions;
mod references;
mod reveal;
mod rows;
mod scroll_area;
mod selection;
mod signal;
mod stops;
mod task_lines;
mod texts;
mod transcript;

pub use attachments::{file_card, image_format};
pub use client_core::calls::agent::shown_kind;
pub use document::Document;
pub use list::{Place, TranscriptList};
pub use proposal::{ICON as PROPOSAL_ICON, ProposalEvent, ProposalRequest, ProposalView};
pub use questions::QuestionPanel;
pub use rows::{
    ABOVE_THE_COMPOSER, ProposalResponseLine, around_column, in_column, proposal_response_line, subagent_standing,
};
pub use selection::{KEY_CONTEXT, Way};
pub use signal::{chat_signal, recorded_work, waiting_on_you, work, workspace_signal};
pub use transcript::{Transcript, TranscriptEvent};

/// What the app adds to the words a field dictated into here favours, such as the workspace's, after the field's own.
type DictationHints = Box<dyn Fn(&mut client_core::hints::HintCollector, &gpui_kit::App)>;

/// How long what's just been asked in the composer's place ignores keys, which a test can wait out.
#[cfg(feature = "testing")]
pub const QUESTION_SETTLES: std::time::Duration = questions::SETTLE;
/// How long what was found of an issue or a pull request a chat links to is drawn before it's asked for again, while
/// it's drawn.
pub const REFERENCES_ASKED_AGAIN_AFTER: std::time::Duration = references::ASK_AGAIN_AFTER;
