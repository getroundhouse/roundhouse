//! What only tests read of a transcript: how much of it is loaded, what's at a row, and what its texts show.

use client_core::session_log::{PromptState, Row};
use protocol::event::{EventBody, SessionEvent};
use protocol::model::Seq;

use super::{Transcript, rows_and_loaded};

impl Transcript {
    /// The event at a row, if it's loaded.
    pub fn event_at(&self, row: usize) -> Option<SessionEvent> {
        self.session.read(|log| match log.row(row) {
            Row::Event(event) => Some(event.clone()),
            Row::Unloaded => None,
        })
    }

    /// Where the prompt recorded at a row stands with the agent, if that row is a recorded prompt that's loaded.
    pub fn prompt_state_at(&self, row: usize) -> Option<PromptState> {
        self.session.read(|log| match log.row(row) {
            Row::Event(event) if matches!(event.body, EventBody::UserPrompt { .. }) => {
                Some(log.prompt_state(event.seq))
            }
            Row::Event(_) | Row::Unloaded => None,
        })
    }

    /// The failed request whose card offers Retry, if one does, as the transcript worked it out last.
    pub fn retry_offered_at(&self) -> Option<Seq> {
        self.model.borrow().retry_at
    }

    /// What the agent is writing, as far as it's got.
    pub fn streaming_text(&self) -> String {
        self.session.read(|log| log.tail().messages.iter().flat_map(|m| &m.blocks).map(|b| b.text.as_str()).collect())
    }

    /// How many rows are loaded as the session's log has them now. The list is told of them when the log says it has
    /// changed, which is after the last of the pages asked for together is in, so this can be ahead of the list.
    pub fn loaded_rows(&self) -> usize {
        self.session.read(|log| rows_and_loaded(log).1.iter().map(ExactSizeIterator::len).sum())
    }

    /// Whether every long text the rows have drawn has been parsed. One is parsed on another thread from the first
    /// time its row is drawn, and its row is measured again once it's in, so until then the session's height isn't
    /// settled.
    pub fn long_texts_parsed(&self) -> bool {
        self.texts.all_parsed()
    }

    /// What a copy holds with everything selected: the chat's Markdown and the plain text its calls show open, in the
    /// order it's read.
    pub fn all_as_copied(&self) -> String {
        self.selection.all_as_copied()
    }

    /// What a copy would hold now, which a test can read while a drag is still going.
    pub fn as_copied(&self) -> String {
        self.selection.as_copied()
    }

    /// What each text the transcript keeps the kit's blocks of was parsed from, which is what it shows.
    pub fn kept_texts(&self) -> Vec<String> {
        self.texts.sources()
    }
}
