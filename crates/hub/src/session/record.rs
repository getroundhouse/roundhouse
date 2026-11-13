//! Writing an event to the log and telling everyone who's watching.

use std::sync::Arc;

use protocol::client::{ApiError, HubNotice};
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ClientMessageId, Session, ToolUseId};
use protocol::tail::Tail;
use tracing::error;

use super::progress::{Next, Step};
use super::{LiveItem, LiveSession, LiveState, Origin, publish};
use crate::hub::Hub;
use crate::store::{Appended, SessionChange, TurnEdge};

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
        note_recorded(origin, &mut change);
        let made = Made::of(&change, waits);
        let id = self.id.clone();
        let client_msg_id = sent_as(&body).cloned();
        let appended = hub
            .blocking(move |store| store.append(&id, run, &body, raw.as_deref(), client_msg_id.as_ref(), Some(change)))
            .await?;
        if appended.duplicate {
            return Ok((appended.event, true));
        }
        self.after_commit(&hub, vec![(origin, &appended, next)], made).await;
        Ok((appended.event, false))
    }

    /// Records a request from the current agent, from `origin`, with the hub's `answer` to it, in one commit, so
    /// nobody is told it's waiting, since nobody is: together they leave the agent where it was, as
    /// [`super::progress::Progress::step_answered`] says, and change nothing on the session's record but how far its
    /// log goes and when the approvals open were last noted.
    pub(crate) async fn record_answered(
        &self,
        origin: Origin,
        (request, raw): (EventBody, Option<String>),
        answer: EventBody,
    ) -> Result<(), ApiError> {
        let _in_order = self.recording.lock().await;
        let hub = self.hub()?;
        let [asked, answered] = self.state.lock().progress.step_answered(request, answer);
        let mut change = asked.change;
        note_recorded(origin, &mut change);
        let made = Made::of(&change, asked.waits || answered.waits);
        let (id, run) = (self.id.clone(), origin.run());
        let (request, answer) = (asked.body, answered.body);
        let both = hub
            .blocking(move |store| store.append_answered(&id, run, (&request, raw.as_deref()), &answer, change))
            .await?;
        let [request, answer] = &both;
        self.after_commit(&hub, vec![(origin, request, asked.next), (Origin::Hub, answer, answered.next)], made).await;
        Ok(())
    }

    /// Takes in what one commit recorded, `recorded`, oldest first, and tells everyone what it changed. Those following
    /// the log are told once for each run of events from the agent or the same subagent, at the newest, so they read
    /// them together.
    async fn after_commit(&self, hub: &Arc<Hub>, recorded: Vec<(Origin, &Appended, Next)>, made: Made) {
        let appended: Vec<&Appended> = recorded.iter().map(|(_, appended, _)| *appended).collect();
        let ready = {
            let mut state = self.state.lock();
            let was_ready = state.ready_for_queue();
            let mut ended_others = false;
            let mut recorded = recorded.into_iter().peekable();
            while let Some((origin, appended, next)) = recorded.next() {
                ended_others |= take(&mut state, origin, appended, next);
                let subagent = &appended.event.subagent;
                if recorded.peek().is_none_or(|(_, after, _)| after.event.subagent != *subagent) {
                    let seq = appended.event.seq;
                    publish(&mut state, LiveItem::Appended { seq, subagent: subagent.clone(), ended_others });
                    ended_others = false;
                }
            }
            !was_ready && state.ready_for_queue()
        };
        if ready {
            self.wake_queue();
        }
        if made.waits {
            self.wait_for_pickup();
        }
        if made.changed {
            hub.work_changed();
        }
        let by_the_person = appended.iter().any(|appended| appended.by_the_person);
        // A prompt sent while the agent works changes no status, but it does change when the person was last active, and
        // a question answered while another is open changes none either, but it takes any answer being typed to it. The
        // feed leaves out a copy whose revision didn't move, so one that took none costs only the read.
        let answered =
            appended.iter().any(|appended| matches!(appended.event.body, EventBody::ApprovalResolved { .. }));
        // What the agent waits on and has yet to take are on the record too, and an event can change them with no
        // change of status, as a prompt taken mid-turn does.
        let waiting_changed =
            appended.iter().any(|appended| appended.untaken.is_some() || appended.approvals.is_some());
        let dequeued = appended.iter().any(|appended| appended.dequeued);
        if (made.changed || dequeued || by_the_person || answered || waiting_changed)
            && let Err(e) = self.announce(hub).await
        {
            error!(session = %self.id, "could not announce a change of status: {e}");
        }
        if by_the_person {
            hub.announce_workspace(&self.workspace_id).await;
        }
        let subagents: Vec<ToolUseId> = appended.iter().flat_map(|appended| appended.subagents.clone()).collect();
        if !subagents.is_empty()
            && let Err(e) = self.announce_subagents(hub, subagents).await
        {
            error!(session = %self.id, "could not announce a change to a subagent: {e}");
        }
        if made.turn_ended {
            hub.read_pull_request_after_turn(&self.workspace_id);
        }
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

/// What a commit's steps made of the session, besides its log, for [`LiveSession::after_commit`] to tell of.
struct Made {
    /// Its record says something new of its status, or of its queue being held.
    changed: bool,
    /// It waits for the agent to pick up again, as [`super::status::after_background`] says.
    waits: bool,
    turn_ended: bool,
}

impl Made {
    fn of(change: &SessionChange, waits: bool) -> Self {
        let turn_ended = change.status.as_ref().is_some_and(|status| matches!(status.turn, TurnEdge::Ends(_)));
        Self { changed: !change.is_none(), waits, turn_ended }
    }
}

/// Has `change` say which of its worker's numbers the agent of `origin` has had recorded, in the same commit as the
/// event, so a hub that connects to the worker again knows what it has.
fn note_recorded(origin: Origin, change: &mut SessionChange) {
    if let Origin::Run { run, numbered: Some(through), .. } = origin {
        change.recorded = Some((run, through));
    }
}

/// Updates live state for an event just recorded, and says whether that ended streamed messages of the agent or of
/// subagents other than the one it came from.
fn take(state: &mut LiveState, origin: Origin, appended: &Appended, next: Next) -> bool {
    let (seq, body, subagent) = (appended.event.seq, &appended.event.body, &appended.event.subagent);
    state.last_seq = seq;
    let others = |tail: &Tail| tail.messages.iter().filter(|message| message.parent_tool_use_id != *subagent).count();
    let others_before = others(&state.tail);
    state.tail.on_event(seq, origin.run(), body);
    let ended_others = others(&state.tail) < others_before;
    state.progress.take(next, appended.event.ts_ms, appended.untaken.clone());
    ended_others
}
