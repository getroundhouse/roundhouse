//! What's come back for a call: its result, found in the log near after it, or the log saying the agent went on
//! without one.

use client_core::calls::{self, Failure, ResultForm, Returned};
use client_core::session_log::SessionLog;
use protocol::capability::calls::CallOutput;
use protocol::event::{ContentBlock, Denied, EventBody, FileChange, Hunk, SessionEvent};
use protocol::model::{Seq, ToolUseId};

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
    pub(super) fn to(log: &SessionLog, m: &Model, seq: Seq, id: &ToolUseId) -> Self {
        let mut dropped = false;
        for event in nearby::events_after(log, seq, NEAR) {
            if let Some(answer) = answer_in(event, id, m) {
                return Reply::Result(answer);
            }
            dropped |= matches!(&event.body, EventBody::ToolCallDropped { tool_use_id } if tool_use_id == id);
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
    /// What it sent back, as text, which [`Answer::shown`] reads for showing.
    text: String,
    /// What it sent back, which a call's row draws by its kind. Boxed, since it's larger than the rest of the answer,
    /// and every [`Reply`] takes the room of one with an answer.
    pub(in crate::rows) output: Box<CallOutput>,
    /// What it changed in a file, when its agent said.
    pub(in crate::rows) change: Option<FileChange>,
    /// The images it sent back, such as a screenshot the agent read.
    pub(in crate::rows) images: Vec<Carried>,
    /// Whether what it sent back is encrypted, for the model alone, so `text` holds none of it.
    pub(in crate::rows) encrypted: bool,
    /// The form it was recorded in, which says how its text is read if it failed.
    form: ResultForm,
    /// The person wouldn't let the tool be used, which isn't a failure, so it's said apart.
    denied: Option<Denied>,
}

/// What became of a call, as its result says.
pub(in crate::rows) enum Outcome<'a> {
    /// It ran, and sent back this.
    Ran(&'a str),
    /// It failed, as this says.
    Failed(Failure<'a>),
    /// The person wouldn't let it be used, which isn't a failure, and it sent back nothing of its own.
    Denied(&'a Denied),
}

impl Answer {
    pub(in crate::rows) fn outcome(&self) -> Outcome<'_> {
        match (&self.denied, self.is_error) {
            (Some(denied), _) => Outcome::Denied(denied),
            (None, true) => Outcome::Failed(Failure::read(&self.text, self.form)),
            (None, false) => Outcome::Ran(&self.text),
        }
    }

    /// What it says went wrong, if it failed.
    pub(in crate::rows) fn failure(&self) -> Option<Failure<'_>> {
        if let Outcome::Failed(failure) = self.outcome() { Some(failure) } else { None }
    }

    /// What it sent back, as it's shown. For one that failed, that's what went wrong without what says how, which the
    /// line under its call says, so a result reads the same whichever form it was recorded in.
    pub(in crate::rows) fn shown(&self) -> &str {
        self.failure().map_or(self.text.as_str(), |failure| failure.output)
    }

    /// What it sent back, if it ran.
    pub(in crate::rows) fn success(&self) -> Option<&str> {
        if let Outcome::Ran(text) = self.outcome() { Some(text) } else { None }
    }

    /// The images it sent back, if it ran.
    pub(in crate::rows) fn images(&self) -> &[Carried] {
        if self.success().is_some() { &self.images } else { &[] }
    }

    /// The text it sent back, unless that's blank and it sent back images too, which are then all there is to show.
    pub(in crate::rows) fn text_beside_images(&self) -> Option<&str> {
        let text = self.shown();
        (!text.trim().is_empty() || self.images().is_empty()).then_some(text)
    }

    /// Whether its call goes on in the background, so what it sent back is only what it had by then, as
    /// [`calls::in_the_background`] says.
    pub(in crate::rows) fn in_the_background(&self) -> bool {
        calls::in_the_background(&self.output)
    }

    /// The parts of a file it changed, when its agent said which they were.
    pub(in crate::rows) fn hunks(&self) -> Option<&[Hunk]> {
        let Some(FileChange::Changed { hunks }) = &self.change else { return None };
        (!hunks.is_empty()).then_some(hunks.as_slice())
    }

    /// What `block` says, if it's a result.
    fn of(block: &ContentBlock, m: &Model) -> Option<Answer> {
        let returned = Returned::of(block)?;
        Some(Answer {
            is_error: returned.is_error,
            change: returned.change.cloned(),
            images: attachments::carried(m, returned.images.iter().copied()),
            encrypted: returned.encrypted,
            output: Box::new(returned.output.clone()),
            form: returned.form(),
            denied: returned.denied.cloned(),
            text: returned.text,
        })
    }
}

/// The result of the call `id`, if it's in `event`. A tool's result comes on the user's side, except one the API ran
/// itself, such as the advisor, whose comes from the agent.
fn answer_in(event: &SessionEvent, id: &ToolUseId, m: &Model) -> Option<Answer> {
    let result = blocks_of(&event.body)
        .iter()
        .find(|block| matches!(block, ContentBlock::CallResult { tool_use_id, .. } if tool_use_id == id))?;
    Answer::of(result, m)
}
