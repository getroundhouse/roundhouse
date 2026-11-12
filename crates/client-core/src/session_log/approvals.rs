//! The approvals still waiting on an answer: as the session's record says, with what's loaded since, or among what a
//! log holds, from a hub too old to say.

use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ApprovalId, OpenApproval, Seq, Session};

use super::SessionLog;

#[cfg(test)]
mod tests;

/// The approvals a copy of a session's record says its agent is waiting on, and the newest event it had recorded then,
/// so what's loaded after that says what has changed since, as a newer copy will once the feed tells of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedApprovals {
    pub open: Vec<OpenApproval>,
    pub through: Seq,
}

impl RecordedApprovals {
    /// What `session` says, or `None` from a hub that doesn't say.
    pub fn of(session: &Session) -> Option<Self> {
        Some(Self { open: session.open_approvals.clone()?, through: session.last_seq })
    }
}

impl SessionLog {
    /// The open approvals whose requests are loaded, oldest first: those `recorded` says are open, with the requests
    /// and answers loaded since. Without a record that says, from a hub too old to, they're found among what's loaded,
    /// as [`SessionLog::open_approvals_loaded`] says.
    pub fn open_approvals(&self, recorded: Option<&RecordedApprovals>) -> Vec<&SessionEvent> {
        let Some(recorded) = recorded else { return self.open_approvals_loaded() };
        let mut open: Vec<&SessionEvent> =
            recorded.open.iter().filter_map(|approval| self.event_of(approval.seq)).collect();
        let since: Vec<&SessionEvent> =
            self.events.values().rev().take_while(|event| event.seq > recorded.through).collect();
        for event in since.into_iter().rev() {
            follow(&mut open, event);
        }
        open
    }

    /// Open approvals among what's loaded. An approval blocks its agent, so an open one is always near the tail, which
    /// is always loaded. Only the events that run unbroken back from the newest count, since an older request's answer
    /// can be in a page that isn't loaded, such as when a page is fetched for scrolling back.
    fn open_approvals_loaded(&self) -> Vec<&SessionEvent> {
        let mut open: Vec<&SessionEvent> = Vec::new();
        for event in self.unbroken_from_newest() {
            follow(&mut open, event);
        }
        open
    }

    /// The loaded events with every event after them loaded too, oldest first.
    fn unbroken_from_newest(&self) -> impl Iterator<Item = &SessionEvent> {
        let mut wanted = self.last;
        let unbroken = |position: &&u64| {
            let next = **position == wanted;
            wanted = position.saturating_sub(1);
            next
        };
        let first = self.events.keys().rev().take_while(unbroken).last().copied();
        first.into_iter().flat_map(|first| self.events.range(first..).map(|(_, event)| event))
    }
}

/// Takes `event` into `open`: a request opens its approval, unless it's open already, and an answer closes it.
fn follow<'a>(open: &mut Vec<&'a SessionEvent>, event: &'a SessionEvent) {
    match &event.body {
        EventBody::ApprovalRequested { approval_id, .. } => {
            if !open.iter().any(|request| asks(request, approval_id)) {
                open.push(event);
            }
        }
        EventBody::ApprovalResolved { approval_id, .. } => open.retain(|request| !asks(request, approval_id)),
        EventBody::UserPrompt { .. }
        | EventBody::CompactionAsked { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::AgentLimits { .. }
        | EventBody::Assistant { .. }
        | EventBody::AgentUser { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::AgentExited { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentMode { .. }
        | EventBody::SubagentStarted { .. }
        | EventBody::SubagentEnded { .. }
        | EventBody::AgentOther { .. }
        | EventBody::AgentCompacting { .. }
        | EventBody::AgentCompacted { .. }
        | EventBody::CacheExpired { .. }
        | EventBody::Unknown => {}
    }
}

/// Whether `request` asks for the approval `id`.
fn asks(request: &SessionEvent, id: &ApprovalId) -> bool {
    matches!(&request.body, EventBody::ApprovalRequested { approval_id, .. } if approval_id == id)
}
