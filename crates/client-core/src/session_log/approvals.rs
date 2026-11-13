//! The approvals still waiting on an answer: as the session's record says, with what's loaded since, or among what a
//! log holds, from a hub too old to say. A request the record names that no page loaded here holds is fetched on its
//! own, so what waits is asked about wherever in the log it was asked.

use std::collections::{BTreeMap, BTreeSet};

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

/// The requests a record says are waiting that no page loaded here holds, each fetched on its own. An open approval
/// is usually near the end, which is always loaded, but a subagent working in the background can have asked long
/// before, with the agent at work since.
#[derive(Debug, Default)]
pub(super) struct Apart {
    /// The requests the record last said are waiting, by their `seq`s.
    wanted: Vec<Seq>,
    fetched: BTreeMap<Seq, SessionEvent>,
    in_flight: BTreeSet<Seq>,
    /// Those whose fetch failed, which aren't asked for again until [`Apart::retry_failed`].
    failed: BTreeSet<Seq>,
}

impl Apart {
    pub(super) fn fetch_failed(&mut self) {
        self.failed.append(&mut self.in_flight);
    }

    pub(super) fn retry_failed(&mut self) {
        self.failed.clear();
    }

    /// Keeps `event`, which its page is letting go of, if it's a request that waits.
    pub(super) fn keep_if_waiting(&mut self, event: SessionEvent) {
        if self.wanted.contains(&event.seq) {
            self.fetched.insert(event.seq, event);
        }
    }

    /// The same requests waiting, with none fetched, for a log that starts again.
    pub(super) fn again(self) -> Self {
        Self { wanted: self.wanted, ..Self::default() }
    }
}

impl SessionLog {
    /// The open approvals whose requests are loaded, or were fetched on their own, oldest first: those `recorded` says
    /// are open, with the requests and answers loaded since. Without a record that says, from a hub too old to,
    /// they're found among what's loaded, as [`SessionLog::open_approvals_loaded`] says.
    pub fn open_approvals(&self, recorded: Option<&RecordedApprovals>) -> Vec<&SessionEvent> {
        let Some(recorded) = recorded else { return self.open_approvals_loaded() };
        let request = |seq: Seq| self.event_of(seq).or_else(|| self.apart.fetched.get(&seq));
        let mut open: Vec<&SessionEvent> = recorded.open.iter().filter_map(|approval| request(approval.seq)).collect();
        let since: Vec<&SessionEvent> =
            self.events.values().rev().take_while(|event| event.seq > recorded.through).collect();
        for event in since.into_iter().rev() {
            follow(&mut open, event);
        }
        open
    }

    /// Notes the requests `recorded` says are waiting, so [`SessionLog::requests_to_fetch`] fetches those no page
    /// loaded here holds, and lets go of any fetched that no longer wait.
    pub(crate) fn waiting_on(&mut self, recorded: Option<&RecordedApprovals>) {
        let wanted: Vec<Seq> = recorded.iter().flat_map(|recorded| &recorded.open).map(|open| open.seq).collect();
        self.apart.fetched.retain(|seq, _| wanted.contains(seq));
        self.apart.wanted = wanted;
    }

    /// The requests to fetch on their own: those waiting that aren't loaded, fetched or on their way, and haven't
    /// failed, which this marks as on their way.
    pub(crate) fn requests_to_fetch(&mut self) -> Vec<Seq> {
        let wanted = self.apart.wanted.clone();
        let missing = |seq: &Seq| self.event_of(*seq).is_none() && !self.apart.fetched.contains_key(seq);
        let to_fetch: Vec<Seq> = wanted
            .into_iter()
            .filter(missing)
            .filter(|seq| !self.apart.in_flight.contains(seq) && !self.apart.failed.contains(seq))
            .collect();
        self.apart.in_flight.extend(&to_fetch);
        to_fetch
    }

    /// Puts the request at `seq`, fetched on its own, in place, or with `None` notes that it couldn't be fetched, so it
    /// isn't asked for again until [`SessionLog::retry_failed`]. One that no longer waits is let go of. A view is told,
    /// so it asks what's waiting again.
    pub(crate) fn install_request(&mut self, seq: Seq, event: Option<SessionEvent>) {
        self.apart.in_flight.remove(&seq);
        if !self.apart.wanted.contains(&seq) {
            return;
        }
        let Some(event) = event.filter(|event| event.seq == seq) else {
            self.apart.failed.insert(seq);
            return;
        };
        self.apart.fetched.insert(seq, event);
        self.changes.requests_fetched = true;
    }

    /// Open approvals among what's loaded, for a hub too old to say. An approval blocks its agent, so one is usually
    /// near the tail, which is always loaded. Only the events that run unbroken back from the newest count, since an
    /// older request's answer can be in a page that isn't loaded, such as when a page is fetched for scrolling back.
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
        | EventBody::RequestFailed { .. }
        | EventBody::AgentMode { .. }
        | EventBody::SubagentStarted { .. }
        | EventBody::SubagentContext { .. }
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
