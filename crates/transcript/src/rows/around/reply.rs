//! What's come back for a call: its result, found in the log near after it, or the log saying the agent went on
//! without one.

use client_core::session_log::SessionLog;
use protocol::event::{ContentBlock, EventBody, FileChange, Hunk, SessionEvent};
use protocol::model::Seq;

use super::{NEAR, blocks_of, nearby};
use crate::attachments::{self, Carried};
use crate::transcript::Model;

/// What's come back for a call.
pub(in crate::rows) enum Reply {
    /// Nothing, so far or ever: its result may be on its way, or its turn may have ended first. A block that isn't a
    /// call or a result has none either.
    Nothing,
    /// Its result, to draw under it.
    Result(Answer),
    /// The agent went on without a result, as the log says with `ToolCallDropped`, so to it the call was never made.
    Dropped,
}

impl Reply {
    /// What's come back for the call `id`, made in the event at `seq`, among the events near after it. A result
    /// stands even after the log says the call was dropped, since what said so only saw what came next.
    pub(super) fn to(log: &SessionLog, m: &Model, seq: Seq, id: &str) -> Self {
        let mut dropped = false;
        for event in nearby::events_after(log, seq, NEAR) {
            if let Some(answer) = answer_in(event, id, m) {
                return Reply::Result(answer);
            }
            dropped |= matches!(&event.body, EventBody::ToolCallDropped { tool_use_id } if tool_use_id.as_str() == id);
        }
        if dropped { Reply::Dropped } else { Reply::Nothing }
    }

    /// `block` itself, if it's a result, for one drawn on its own.
    pub(super) fn of(block: &ContentBlock, m: &Model) -> Self {
        Answer::of(block, m).map_or(Reply::Nothing, Reply::Result)
    }

    pub(in crate::rows) fn result(&self) -> Option<&Answer> {
        if let Reply::Result(answer) = self { Some(answer) } else { None }
    }
}

/// A call's result, found in the log, to draw under the call.
pub(in crate::rows) struct Answer {
    pub(in crate::rows) is_error: bool,
    pub(in crate::rows) text: String,
    /// What it changed in a file, when its agent said.
    pub(in crate::rows) change: Option<FileChange>,
    /// The images it sent back, such as a screenshot the agent read.
    pub(in crate::rows) images: Vec<Carried>,
    /// Whether what it sent back is encrypted, for the model alone, so `text` holds none of it.
    pub(in crate::rows) encrypted: bool,
}

impl Answer {
    /// What it says went wrong, if it failed.
    pub(in crate::rows) fn failure(&self) -> Option<&str> {
        self.is_error.then_some(self.text.as_str())
    }

    /// What it sent back, if it didn't fail.
    pub(in crate::rows) fn success(&self) -> Option<&str> {
        (!self.is_error).then_some(self.text.as_str())
    }

    /// The images it sent back, if it didn't fail.
    pub(in crate::rows) fn images(&self) -> &[Carried] {
        if self.is_error { &[] } else { &self.images }
    }

    /// The text it sent back, unless that's blank and it sent back images too, which are then all there is to show.
    pub(in crate::rows) fn text_beside_images(&self) -> Option<&str> {
        (!self.text.trim().is_empty() || self.images().is_empty()).then_some(self.text.as_str())
    }

    /// The parts of a file it changed, when its agent said which they were.
    pub(in crate::rows) fn hunks(&self) -> Option<&[Hunk]> {
        let Some(FileChange::Changed { hunks }) = &self.change else { return None };
        (!hunks.is_empty()).then_some(hunks.as_slice())
    }

    /// What `block` says, if it's a result.
    fn of(block: &ContentBlock, m: &Model) -> Option<Answer> {
        let ContentBlock::ToolResult { is_error, text, change, images, encrypted, .. } = block else { return None };
        let images = attachments::carried(m, images);
        Some(Answer { is_error: *is_error, text: text.clone(), change: change.clone(), images, encrypted: *encrypted })
    }
}

/// The result of the call `id`, if it's in `event`. A tool's result comes on the user's side, except one the API ran
/// itself, such as the advisor, whose comes from the agent.
fn answer_in(event: &SessionEvent, id: &str, m: &Model) -> Option<Answer> {
    let result = blocks_of(&event.body)
        .iter()
        .find(|block| matches!(block, ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id.as_str() == id))?;
    Answer::of(result, m)
}
