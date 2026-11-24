use super::super::SessionLog;
use protocol::event::{ApprovalChange, EventBody, SessionEvent};
use protocol::model::{ApprovalId, OpenApproval, Seq, Session};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// The approvals a copy of a session's record says its agent is waiting on, and the newest event it had recorded then,
/// so what's loaded after that says what has changed since, as a newer copy will once the feed tells of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordedApprovals {
    pub open: Vec<OpenApproval>,
    pub through: Seq,
}

impl RecordedApprovals {
    /// What `session` says.
    pub fn of(session: &Session) -> Self {
        Self { open: session.open_approvals.clone(), through: session.last_seq }
    }
}

/// The requests a record says are waiting that no page loaded here holds, each fetched on its own. An open approval
/// is usually near the end, which is always loaded, but a subagent working in the background can have asked long
/// before, with the agent at work since.
#[derive(Clone, Debug, Default)]
pub(in super::super) struct Apart {
    /// The requests the record last said are waiting, by their `seq`s.
    pub(super) wanted: Vec<Seq>,
    pub(super) fetched: BTreeMap<Seq, Arc<SessionEvent>>,
    pub(super) in_flight: BTreeSet<Seq>,
    /// Those whose fetch failed, which aren't asked for again until [`Apart::retry_failed`].
    pub(super) failed: BTreeSet<Seq>,
    /// Those fetched that are to be fetched again, as [`SessionLog::read_again`] says, and stay drawn until then.
    pub(super) stale: BTreeSet<Seq>,
}

impl Apart {
    pub(in super::super) fn fetch_failed(&mut self) {
        self.failed.append(&mut self.in_flight);
    }

    pub(in super::super) fn retry_failed(&mut self) {
        self.failed.clear();
    }

    /// Keeps `event`, which its page is letting go of, if it's a request that waits, to be fetched again if the page
    /// was.
    pub(in super::super) fn keep_if_waiting(&mut self, event: Arc<SessionEvent>, stale: bool) {
        if self.wanted.contains(&event.seq) {
            if stale {
                self.stale.insert(event.seq);
            }
            self.fetched.insert(event.seq, event);
        }
    }

    /// Fetches again each request fetched, keeping it until it comes.
    pub(in super::super) fn read_again(&mut self) {
        self.stale = self.fetched.keys().copied().collect();
    }
}

impl SessionLog {
    /// The open approvals whose requests are loaded, or were fetched on their own, oldest first: those `recorded` says
    /// are open, with the requests and answers loaded since.
    pub fn open_approvals(&self, recorded: &RecordedApprovals) -> Vec<&SessionEvent> {
        let request = |seq: Seq| self.event_of(seq).or_else(|| self.apart.fetched.get(&seq).map(Arc::as_ref));
        let mut open: Vec<&SessionEvent> = recorded.open.iter().filter_map(|approval| request(approval.seq)).collect();
        let since: Vec<&SessionEvent> =
            self.events.values().map(Arc::as_ref).rev().take_while(|event| event.seq > recorded.through).collect();
        for event in since.into_iter().rev() {
            follow(&mut open, event);
        }
        open
    }

    /// Notes the requests `recorded` says are waiting, so [`SessionLog::requests_to_fetch`] fetches those no page
    /// loaded here holds, and lets go of any fetched that no longer wait.
    pub(crate) fn waiting_on(&mut self, recorded: &RecordedApprovals) {
        let wanted: Vec<Seq> = recorded.open.iter().map(|open| open.seq).collect();
        self.apart.fetched.retain(|seq, _| wanted.contains(seq));
        self.apart.stale.retain(|seq| wanted.contains(seq));
        self.apart.wanted = wanted;
    }

    /// The requests to fetch on their own: those waiting that aren't loaded or fetched, or are to be fetched again, and
    /// aren't on their way and haven't failed, which this marks as on their way.
    pub(crate) fn requests_to_fetch(&mut self) -> Vec<Seq> {
        let wanted = self.apart.wanted.clone();
        let held = |seq: &Seq| self.event_of(*seq).is_some() || self.apart.fetched.contains_key(seq);
        let missing = |seq: &Seq| !held(seq) || self.apart.stale.contains(seq);
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
        self.apart.stale.remove(&seq);
        self.apart.fetched.insert(seq, Arc::new(event));
        self.changes.requests_fetched = true;
    }
}

/// Takes `event` into `open`, as [`EventBody::approval_change`] says: a request opens its approval, unless it's open
/// already, and an answer closes it.
fn follow<'a>(open: &mut Vec<&'a SessionEvent>, event: &'a SessionEvent) {
    match event.body.approval_change() {
        Some(ApprovalChange::Opens { approval_id, .. }) => {
            if !open.iter().any(|request| asks(request, approval_id)) {
                open.push(event);
            }
        }
        Some(ApprovalChange::Closes(approval_id)) => open.retain(|request| !asks(request, approval_id)),
        None => {}
    }
}

/// Whether `request` asks for the approval `id`.
fn asks(request: &SessionEvent, id: &ApprovalId) -> bool {
    matches!(&request.body, EventBody::ApprovalRequested { approval_id, .. } if approval_id == id)
}
