//! The agent's own failed requests in a row, each the same failure as the one before, which the hub says of each as it
//! records it, so every client shows them as one.

use protocol::event::{Again, EventBody, RequestFailure, TurnOutcome};
use protocol::model::{Seq, SessionId};

use crate::store::{Places, Store, StoreError};

/// How many of the agent's own events [`Failing::recorded`] reads at once, and at most how many bytes. What ends the
/// row, such as the next prompt, nearly always comes within a few events of the failure, or the log ends first, so a
/// small read is enough, and one past a few turns of a busy agent's events, each a few hundred bytes, is rare.
pub(crate) const READ_AT_ONCE: (u32, u64) = (64, 256 * 1024);

/// The agent's own newest failed request, while nothing since has ended its row, as [`keeps`] says: what failed,
/// where it's recorded, and how many times in a row it has.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Failing {
    pub(super) failure: RequestFailure,
    pub(super) last: Seq,
    pub(super) times: u32,
}

/// What recording an event does to the failures in a row, for [`super::Progress::take`] once the event's seq is known.
#[derive(Debug)]
pub(in super::super) enum FailingAfter {
    /// It leaves them as they were.
    Keeps,
    /// It ends the row.
    Ends,
    /// It's the agent's own failed request, failing for the `times`th time in a row.
    Fails { failure: RequestFailure, times: u32 },
}

impl Failing {
    /// That the agent's own failed request `failure`, recorded after these, repeats them, if it's the same failure.
    fn again(&self, failure: &RequestFailure) -> Option<Again> {
        (self.failure == *failure).then_some(Again { previous: self.last, times: self.times.saturating_add(1) })
    }

    /// The failures in a row that session `id`'s log ends in, if nothing recorded since the agent's own newest failed
    /// request ended the row, so a session started again between two failures, as when the hub starts again, still
    /// says the second repeats the first. Only the agent's own events are read, since none of a subagent's ends a row.
    pub(crate) fn recorded(store: &Store, id: &SessionId) -> Result<Option<Self>, StoreError> {
        let Some(newest) = store.newest_own_failure(id)? else { return Ok(None) };
        let EventBody::RequestFailed { failure, again, .. } = newest.body else { return Ok(None) };
        let (limit, bytes) = READ_AT_ONCE;
        let mut from = newest.place.next();
        loop {
            let places = Places { subagent: None, from, through: None };
            let (since, _) = store.read_places(id, places, limit, bytes, false)?;
            let Some(last) = since.last() else { break };
            if !since.iter().all(|event| keeps(&event.body)) {
                return Ok(None);
            }
            from = last.place.next();
        }
        Ok(Some(Self { failure, last: newest.seq, times: again.map_or(1, |again| again.times) }))
    }
}

/// What recording `body` does to the failures in a row, `failing`, setting its `again` if it's the agent's own failed
/// request and repeats them. Only the hub says what one repeats, so a subagent's says nothing, whatever it came with.
pub(super) fn after(failing: Option<&Failing>, body: &mut EventBody) -> FailingAfter {
    if let EventBody::RequestFailed { subagent, failure, again } = body {
        if subagent.is_some() {
            *again = None;
            return FailingAfter::Keeps;
        }
        *again = failing.and_then(|failing| failing.again(failure));
        return FailingAfter::Fails { failure: failure.clone(), times: again.map_or(1, |again| again.times) };
    }
    if keeps(body) { FailingAfter::Keeps } else { FailingAfter::Ends }
}

/// Whether the agent's own failed requests either side of `body` can still be failures in a row. Everything of a
/// subagent's keeps them, since the subagent's tab shows it, and so does what the agent does by itself that comes to
/// nothing else: starting a turn, such as to take in a task's end, saying what it's running, what changed of one of its
/// tasks or of its limits, a line its adapter has no form for, and ending a turn that only failed or said nothing. What the log records of the person
/// ends the row, such as a prompt, Retry among them, a change of approval mode or a turn they stopped, and so does
/// anything the agent says or does, the agent going or starting, and anything this build can't tell.
pub(super) fn keeps(body: &EventBody) -> bool {
    if body.subagent().is_some() {
        return true;
    }
    match body {
        EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::OwnTurn(_)
        | EventBody::AgentLimits { .. }
        | EventBody::TaskChanged { .. }
        | EventBody::AgentOther { .. }
        | EventBody::SubagentContext { .. }
        | EventBody::SubagentEnded { .. } => true,
        // A turn the agent started by itself can end with nothing said, as when its CLI doesn't try while it's
        // limited, and anything it said would have ended the row already.
        EventBody::TurnEnded { outcome, request_failed, .. } => *request_failed || *outcome == TurnOutcome::Success,
        EventBody::UserPrompt { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::CompactionAsked { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentExited { .. }
        | EventBody::Assistant { .. }
        | EventBody::AgentUser { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::RequestFailed { .. }
        | EventBody::ApprovalRequested { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentMode { .. }
        | EventBody::SubagentStarted { .. }
        | EventBody::AgentCompacting { .. }
        | EventBody::AgentCompacted { .. }
        | EventBody::CacheExpired { .. }
        | EventBody::ConversationStarted { .. }
        | EventBody::Unknown => false,
    }
}
