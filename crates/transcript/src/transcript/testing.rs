//! What only tests read of a transcript: how much of it is loaded, what's at a row and which row has what, and what its
//! texts show.

use client_core::session_log::{By, PromptState, Row};
use gpui_kit::{Bounds, Context, Pixels, Window};
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{PromptLine, Seq};

use super::{Transcript, rows_and_loaded};

impl Transcript {
    /// The event at a row, if it's loaded.
    pub fn event_at(&self, row: usize) -> Option<SessionEvent> {
        self.session.read(|log| match log.row(row) {
            Row::Event(event) => Some(event.clone()),
            Row::Unloaded => None,
        })
    }

    /// The row `event` is in, whether it's loaded or not, as its position in the log says, or `None` for one the
    /// transcript doesn't follow. A log by `seq` follows every event, and one by place follows the agent's own in a
    /// chat, and in the subagent tab, the subagent's.
    pub fn event_row(&self, event: &SessionEvent) -> Option<usize> {
        let followed = event.subagent == self.model.borrow().alone;
        self.session.read(|log| match log.by() {
            By::Place if !followed => None,
            By::Seq | By::Place => log.position(event).row(),
        })
    }

    /// The row the outline puts the prompt `line` in, as the log says.
    pub fn prompt_row(&self, line: &PromptLine) -> usize {
        self.session.read(|log| log.prompt_row(line))
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

    /// How many rows are loaded as the session's log has them now. The list is told of them as the transcript catches
    /// up with the log, once the log says it has changed, as it does when each page is put in place, so this can be
    /// ahead of the list.
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

    /// Starts a frame as the list does, for a test that then draws its rows one at a time with
    /// [`Transcript::draw_row_in_this_frame`], so the log can change between them, as it can while a frame is drawn.
    pub fn start_a_frame(&mut self) {
        self.texts.new_frame();
    }

    /// Draws the list's row `ix` as a frame does, in the frame drawn last or started by
    /// [`Transcript::start_a_frame`], for a test of what it shows. Nothing else is drawn.
    pub fn draw_row_in_this_frame(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let row = self.frame_rows(look::LineText::new(window, cx), cx);
        row(ix, cx);
    }

    /// What each text the transcript keeps the kit's blocks of was parsed from, which is what it shows.
    pub fn kept_texts(&self) -> Vec<String> {
        self.texts.sources()
    }

    /// Where each leaf of text drawn in the last frame was laid out, in the order they were drawn.
    pub fn drawn_leaf_bounds(&self) -> Vec<Bounds<Pixels>> {
        self.selection.drawn_leaf_bounds()
    }
}
