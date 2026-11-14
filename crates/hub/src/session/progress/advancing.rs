use super::super::Origin;
use super::super::queue::QueueWatch;
use super::super::status::{Approvals, Open, after_background, next_status, open_after, turn_edge};
use crate::store::{SessionChange, StatusChange, Store, StoreError, TurnEdge};
use protocol::event::{AgentTask, EventBody};
use protocol::model::{ApprovalId, RunId, Seq, Session, SessionId, SessionStatus};

/// Where the session's agent has got to, as its record says: everything that recording an event changes together.
/// [`Progress::step`] is the only thing that works out a change to any of it, whether the session is live or the hub is
/// writing straight to the store before it serves, so a new kind of event, or a new piece of this, goes there. A request
/// the hub answers as it's recorded is the one exception, which [`Progress::step_answered`] says changes nothing.
#[derive(Debug)]
pub(crate) struct Progress {
    pub(in super::super) status: SessionStatus,
    pub(in super::super) approvals: Approvals,
    /// What the current agent is running in the background, from what the session's record lists, since a wait the
    /// hub keeps outlives the agent, though nothing else does.
    pub(in super::super) background_tasks: Vec<AgentTask>,
    /// When the newest turn started, for the turn's end to carry.
    pub(in super::super) turn_started_at_ms: Option<i64>,
    /// The prompts the agent hasn't taken, and whether the queue is held.
    pub(in super::super) queue: QueueWatch,
    /// Whether the session's only work is a wait the hub keeps, as this hub stops, so its agent's exit cuts nothing
    /// off.
    pub(in super::super) outlives_this_hub: bool,
}

/// What recording an event does, worked out before it's recorded: the event as it's recorded, what it changes on the
/// session's record, which commits with it, and where that leaves the agent, for [`Progress::take`] once it's committed.
pub(in super::super) struct Step {
    pub(in super::super) body: EventBody,
    pub(in super::super) change: SessionChange,
    pub(in super::super) next: Next,
    /// Whether it leaves the session waiting for the agent to pick up again, as [`after_background`] says.
    pub(in super::super) waits: bool,
}

/// Where the agent has got to once an event is recorded, as far as that's known before it is.
#[derive(Debug)]
pub(in super::super) struct Next {
    pub(super) status: SessionStatus,
    pub(super) turn: TurnEdge,
    pub(super) approvals: Approvals,
    pub(super) background_tasks: Vec<AgentTask>,
    pub(super) held: Option<bool>,
}

impl Progress {
    /// As `session`'s record says, with the approvals still open, in the order they were asked, each by `asked_by`, and
    /// the prompts its agent hasn't taken.
    pub(crate) fn recorded(
        session: &Session,
        open_approvals: &[ApprovalId],
        asked_by: Option<RunId>,
        untaken: Vec<Seq>,
    ) -> Self {
        let mut queue = QueueWatch::starting(session.queue_held);
        queue.set_untaken(untaken);
        Self {
            status: session.status,
            approvals: open_approvals.iter().map(|id| (id.clone(), asked_by)).collect(),
            background_tasks: session.background_tasks.clone(),
            turn_started_at_ms: session.turn_started_at_ms,
            queue,
            outlives_this_hub: false,
        }
    }

    /// What recording `body`, from `origin`, does: to the status, the turns, the approvals open, what's running in the
    /// background, and whether the queue is held. `agent` says whether an agent is running, so it can pick up again by
    /// itself. Only the hub and the current agent speak for the session, so what a replaced agent says changes nothing
    /// but which approvals are open, and one it asks stays open until it's cancelled. As the hub stops, the agent of a
    /// session whose only work is a wait the next hub keeps cuts nothing off by going, so the session stays at work in
    /// the background, with the wait listed and its queue free. A turn's end from the agent says when the hub recorded
    /// that turn's start, which the agent can't know, so anything it said is replaced.
    pub(in super::super) fn step(&self, origin: Origin, mut body: EventBody, agent: bool) -> Step {
        let (approvals, background_tasks) = open_after(&self.approvals, &self.background_tasks, origin.run(), &body);
        let speaks = origin.speaks_for_session();
        let outlived = speaks && self.outlives_this_hub && matches!(body, EventBody::AgentExited { .. });
        if !speaks || outlived {
            let held = (outlived && self.queue.is_held()).then_some(false);
            let change = SessionChange { queue_held: held, ..SessionChange::default() };
            let background_tasks = self.background_tasks.clone();
            let next = Next { status: self.status, turn: TurnEdge::Neither, approvals, background_tasks, held };
            return Step { body, change, next, waits: false };
        }
        if let EventBody::TurnEnded { started_at_ms, .. } = &mut body {
            *started_at_ms = self.turn_started_at_ms;
        }
        let open = Open { approvals: approvals.len(), tasks: background_tasks.len(), agent };
        let status = next_status(self.status, &body, open, self.queue.idle_is_stale());
        let turn = turn_edge(self.status, status, &body);
        let changed_tasks = (background_tasks != self.background_tasks).then(|| background_tasks.clone());
        let changed = status != self.status || changed_tasks.is_some();
        let held = self.queue.held_after(&body, self.status, status);
        let change = SessionChange {
            initialized: None,
            claims: None,
            status: changed.then_some(StatusChange { status, turn, background_tasks: changed_tasks }),
            queue_held: held,
            recorded: None,
        };
        let waits = after_background(self.status, status, self.background_tasks.len(), background_tasks.len());
        Step { body, change, next: Next { status, turn, approvals, background_tasks, held }, waits }
    }

    /// What recording `request`, from the current agent, does when the hub records its `answer` in the same commit,
    /// and what recording that answer does: nothing but the log, since nobody waited on it. The agent is where it was,
    /// with the same approvals open and the same work in the background. Taken one at a time by [`Progress::step`],
    /// the answer would end a wait that never began, and have an agent at work in the background running.
    pub(in super::super) fn step_answered(&self, request: EventBody, answer: EventBody) -> [Step; 2] {
        let unmoved = |body| Step {
            body,
            change: SessionChange::default(),
            next: Next {
                status: self.status,
                turn: TurnEdge::Neither,
                approvals: self.approvals.clone(),
                background_tasks: self.background_tasks.clone(),
                held: None,
            },
            waits: false,
        };
        [unmoved(request), unmoved(answer)]
    }

    /// Takes the step an event made, once it's committed at `ts_ms`, and the prompts the agent hasn't taken, if it
    /// changed them. The same step decided what was committed, so this is what the record now says.
    pub(in super::super) fn take(&mut self, next: Next, ts_ms: i64, untaken: Option<Vec<Seq>>) {
        if next.turn == TurnEdge::Starts {
            self.turn_started_at_ms = Some(ts_ms);
        }
        self.status = next.status;
        self.approvals = next.approvals;
        self.background_tasks = next.background_tasks;
        if let Some(untaken) = untaken {
            self.queue.set_untaken(untaken);
        }
        if let Some(held) = next.held {
            self.queue.hold(held);
        }
    }

    /// Whether the session is at work in the background with nothing listed there, as when it waits for the agent to
    /// pick up again, as [`after_background`] says.
    pub(in super::super) fn waits_for_pickup(&self) -> bool {
        self.status == SessionStatus::Background && self.background_tasks.is_empty()
    }

    /// As this hub stops, or as the next finds the agent gone, with a wait the hub keeps open if `waiting`: has the
    /// agent's exit cut nothing off if the session's only work is that wait, as [`Progress::step`] says.
    pub(crate) fn outlive_this_hub(&mut self, waiting: bool) {
        self.outlives_this_hub = waiting && self.status.only_waits(&self.background_tasks);
    }

    /// Records `body` as the hub's in session `id`, which has no live copy and no agent running, as before the hub
    /// serves, and takes the step.
    pub(crate) fn record_in(&mut self, store: &Store, id: &SessionId, body: EventBody) -> Result<(), StoreError> {
        let Step { body, change, next, .. } = self.step(Origin::Hub, body, false);
        let appended = store.append(id, None, &body, None, None, Some(change))?;
        self.take(next, appended.event.ts_ms, appended.untaken);
        Ok(())
    }
}
