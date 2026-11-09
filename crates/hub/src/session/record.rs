//! Writing an event to the log and telling everyone who's watching.

use protocol::client::{ApiError, HubNotice};
use protocol::event::{AgentTask, EventBody, SessionEvent};
use protocol::model::{ClientMessageId, Session, SessionStatus};
use tracing::error;

use super::status::{ends_turn, next_status, open_after};
use super::{LiveItem, LiveSession, LiveState, Origin, publish};
use crate::hub::Hub;
use crate::store::{Appended, SessionChange, StatusChange};

/// The status and the work in the background once `body` is applied, and what of that and the queue's hold has
/// changed, to commit with it. Only the hub and the current agent speak for the session.
fn after(state: &LiveState, origin: Origin, body: &EventBody) -> (SessionStatus, SessionChange, Vec<AgentTask>) {
    if !origin.speaks_for_session() {
        return (state.status, SessionChange::default(), state.background_tasks.clone());
    }
    let (open, tasks) = open_after(&state.pending_approvals, &state.background_tasks, body);
    let next = next_status(state.status, body, open, state.queue.idle_is_stale(origin));
    let ends_turn = ends_turn(state.status, next);
    let background_tasks = (tasks != state.background_tasks).then(|| tasks.clone());
    let changed = next != state.status || background_tasks.is_some();
    let status = changed.then_some(StatusChange { status: next, ends_turn, background_tasks });
    (next, SessionChange { status, queue_held: state.queue.held_after(body, state.status, next) }, tasks)
}

impl LiveSession {
    /// Commits to the log first, so nothing is seen that isn't durable, then updates live state.
    /// One at a time: both tasks record, and status has to change in the same order as the log. Anything the hub has
    /// to say about the agent's prompts for it, because the agent can't, goes first.
    pub(crate) async fn record(
        &self,
        origin: Origin,
        body: EventBody,
        raw: Option<String>,
        client_msg_id: Option<ClientMessageId>,
    ) -> Result<(SessionEvent, bool), ApiError> {
        let _in_order = self.recording.lock().await;
        let settled = self.state.lock().queue.settled_before(origin, &body);
        for verdict in settled {
            self.record_in_order(Origin::Hub, verdict, None, None).await?;
        }
        self.record_in_order(origin, body, raw, client_msg_id).await
    }

    /// Records one event. The caller holds `recording`.
    async fn record_in_order(
        &self,
        origin: Origin,
        body: EventBody,
        raw: Option<String>,
        client_msg_id: Option<ClientMessageId>,
    ) -> Result<(SessionEvent, bool), ApiError> {
        let hub = self.hub()?;
        let run = origin.run();
        let (next, change, tasks) = after(&self.state.lock(), origin, &body);
        let (announce, held) = (!change.is_none(), change.queue_held);
        let id = self.id.clone();
        let for_store = body.clone();
        let appended = hub
            .blocking(move |store| {
                store.append(&id, run, &for_store, raw.as_deref(), client_msg_id.as_ref(), Some(change))
            })
            .await?;
        if appended.duplicate {
            return Ok((appended.event, true));
        }
        if apply(&mut self.state.lock(), origin, &appended, next, tasks, held) {
            self.wake_queue();
        }
        if (announce || appended.dequeued)
            && let Err(e) = self.announce(&hub).await
        {
            error!(session = %self.id, "could not announce a change of status: {e}");
        }
        Ok((appended.event, false))
    }

    /// Tells everyone following the feed what this session is now. It's read back from the store
    /// rather than put together here, so what's sent and its revision always belong together.
    pub(crate) async fn announce(&self, hub: &Hub) -> Result<Session, ApiError> {
        let id = self.id.clone();
        let session = hub.blocking(move |store| store.get_session(&id)).await?.session;
        hub.notify(HubNotice::Session(session.clone()));
        Ok(session)
    }

    /// For where nobody is waiting to hear that recording failed.
    pub(crate) async fn record_or_log(&self, origin: Origin, body: EventBody) {
        if let Err(e) = self.record(origin, body, None, None).await {
            error!(session = %self.id, "could not record an event: {e}");
        }
    }
}

/// Updates live state for an event just recorded, and says whether that has just made the agent ready for the next
/// queued prompt.
fn apply(
    state: &mut LiveState,
    origin: Origin,
    appended: &Appended,
    next: SessionStatus,
    tasks: Vec<AgentTask>,
    held: Option<bool>,
) -> bool {
    let (seq, body) = (appended.event.seq, &appended.event.body);
    let closed = state.closed_at_ms.is_some();
    let was_ready = state.queue.ready(state.status, closed);
    state.last_seq = seq;
    state.tail.on_event(origin.run(), body);
    match body {
        EventBody::ApprovalRequested { approval_id, .. } => {
            state.pending_approvals.insert(approval_id.clone(), origin.run());
        }
        EventBody::ApprovalResolved { approval_id, .. } => {
            state.pending_approvals.remove(approval_id);
        }
        EventBody::UserPrompt { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::Assistant { .. }
        | EventBody::AgentUser { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::AgentExited { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentMode { .. }
        | EventBody::AgentOther { .. }
        | EventBody::Unknown => {}
    }
    state.status = next;
    state.background_tasks = tasks;
    state.queue.heard(origin, body);
    if let Some(untaken) = &appended.untaken {
        state.queue.set_untaken(untaken.clone());
    }
    if let Some(held) = held {
        state.queue.hold(held);
    }
    publish(state, LiveItem::Appended(seq));
    !was_ready && state.queue.ready(state.status, closed)
}
