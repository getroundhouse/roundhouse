//! Writing an event to the log and telling everyone who's watching.

use protocol::client::{ApiError, HubNotice};
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ClientMessageId, Session, ToolUseId};
use protocol::tail::Tail;
use tracing::error;

use super::progress::{Next, Step};
use super::{LiveItem, LiveSession, LiveState, Origin, publish};
use crate::hub::Hub;
use crate::store::{Appended, TurnEdge};

/// The ID a prompt was sent under, which the store records it once under.
fn sent_as(body: &EventBody) -> Option<&ClientMessageId> {
    let EventBody::UserPrompt { client_msg_id, .. } = body else { return None };
    client_msg_id.as_ref()
}

impl LiveSession {
    /// Commits to the log first, so nothing is seen that isn't durable, then updates live state.
    /// One at a time: both tasks record, and status has to change in the same order as the log.
    pub(crate) async fn record(
        &self,
        origin: Origin,
        body: EventBody,
        raw: Option<String>,
    ) -> Result<(SessionEvent, bool), ApiError> {
        self.record_unless(origin, body, raw, |_| None).await
    }

    /// Records `body` unless `refused` says why not, of the session as everything recorded before it left it. It's
    /// asked in the same order as everything is recorded, so nothing recorded meanwhile can make its answer wrong. A
    /// prompt is recorded once under its `client_msg_id`, and recording it again returns the first, as a duplicate.
    pub(super) async fn record_unless(
        &self,
        origin: Origin,
        body: EventBody,
        raw: Option<String>,
        refused: impl FnOnce(&LiveState) -> Option<ApiError>,
    ) -> Result<(SessionEvent, bool), ApiError> {
        let _in_order = self.recording.lock().await;
        let hub = self.hub()?;
        let run = origin.run();
        let Step { body, mut change, next, waits } = {
            let state = self.state.lock();
            if let Some(refusal) = refused(&state) {
                return Err(refusal);
            }
            // The current agent saying it, which only a running one can, or the hub with one running.
            let agent = state.agent.is_some() || run.is_some();
            state.progress.step(origin, body, agent)
        };
        // In the same commit as the event, so a hub that connects to the worker again knows what it has.
        if let Origin::Run { run, numbered: Some(through), .. } = origin {
            change.recorded = Some((run, through));
        }
        let announce = !change.is_none();
        let turn_ended = change.status.as_ref().is_some_and(|status| status.turn == TurnEdge::Ends);
        let id = self.id.clone();
        let client_msg_id = sent_as(&body).cloned();
        let for_store = body.clone();
        let appended = hub
            .blocking(move |store| {
                store.append(&id, run, &for_store, raw.as_deref(), client_msg_id.as_ref(), Some(change))
            })
            .await?;
        if appended.duplicate {
            return Ok((appended.event, true));
        }
        if apply(&mut self.state.lock(), origin, &appended, next) {
            self.wake_queue();
        }
        if waits {
            self.wait_for_pickup();
        }
        if announce {
            hub.work_changed();
        }
        // A prompt sent while the agent works changes no status, but it does change when the person was last active, and
        // a question answered while another is open changes none either, but it takes any answer being typed to it. The
        // feed leaves out a copy whose revision didn't move, so one that took none costs only the read.
        let answered = matches!(appended.event.body, EventBody::ApprovalResolved { .. });
        if (announce || appended.dequeued || appended.by_the_person || answered)
            && let Err(e) = self.announce(&hub).await
        {
            error!(session = %self.id, "could not announce a change of status: {e}");
        }
        if appended.by_the_person {
            hub.announce_workspace(&self.workspace_id).await;
        }
        if !appended.subagents.is_empty()
            && let Err(e) = self.announce_subagents(&hub, appended.subagents.clone()).await
        {
            error!(session = %self.id, "could not announce a change to a subagent: {e}");
        }
        if turn_ended {
            hub.read_pull_request_after_turn(&self.workspace_id);
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

    /// Tells everyone following the feed what the records of the subagents that made `calls` are now, read back from
    /// the store as the session is.
    async fn announce_subagents(&self, hub: &Hub, calls: Vec<ToolUseId>) -> Result<(), ApiError> {
        let id = self.id.clone();
        let subagents = hub
            .blocking(move |store| {
                let records = calls.iter().map(|call| store.get_subagent(&id, call));
                records.filter_map(Result::transpose).collect::<Result<Vec<_>, _>>()
            })
            .await?;
        for subagent in subagents {
            hub.notify(HubNotice::Subagent(subagent));
        }
        Ok(())
    }

    /// For where nobody is waiting to hear that recording failed.
    pub(crate) async fn record_or_log(&self, origin: Origin, body: EventBody) {
        if let Err(e) = self.record(origin, body, None).await {
            error!(session = %self.id, "could not record an event: {e}");
        }
    }
}

/// Updates live state for an event just recorded, and says whether that has just made the agent ready for the next
/// queued prompt.
fn apply(state: &mut LiveState, origin: Origin, appended: &Appended, next: Next) -> bool {
    let (seq, body, subagent) = (appended.event.seq, &appended.event.body, &appended.event.subagent);
    let was_ready = state.ready_for_queue();
    state.last_seq = seq;
    let others = |tail: &Tail| tail.messages.iter().filter(|message| message.parent_tool_use_id != *subagent).count();
    let others_before = others(&state.tail);
    state.tail.on_event(seq, origin.run(), body);
    let ended_others = others(&state.tail) < others_before;
    state.progress.take(next, appended.event.ts_ms, appended.untaken.clone());
    publish(state, LiveItem::Appended { seq, subagent: subagent.clone(), ended_others });
    !was_ready && state.ready_for_queue()
}
