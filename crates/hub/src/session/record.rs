//! Writing an event to the log and telling everyone who's watching.

use std::sync::Arc;

use protocol::capability::approvals::ApprovalMode;
use protocol::capability::settings::Settings;
use protocol::client::{ApiError, HubNotice};
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ClientMessageId, Interaction, InteractionMode, RunCapabilities, RunId, Session, SubagentId};
use protocol::tail::Tail;
use tracing::error;

use super::conversation::{left_conversation, set_interaction};
use super::progress::{Next, Step};
use super::{LiveItem, LiveSession, LiveState, Origin, publish};
use crate::hub::{Hub, Keeper};
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
        self.record_in_order(origin, body, raw, |state| refused(state).map_or(Ok(Admitted::default()), Err)).await
    }

    /// Records `body`, the hub's word that the person changed the chat's mode, with the model, mode and effort they
    /// changed, `settings`, which are saved in the same commit, so no hub that stops between two writes can leave the
    /// record in a mode the log never says it changed to.
    pub(super) async fn record_settings(&self, body: EventBody, settings: Settings) -> Result<(), ApiError> {
        let _in_order = self.recording.lock().await;
        let admitted = Admitted { settings: Some(settings), ..Admitted::default() };
        self.record_in_order(Origin::Hub, body, None, |_| Ok(admitted)).await?;
        Ok(())
    }

    /// Records `body` as [`LiveSession::record_unless`] does, for a caller that holds `recording` already, so it can
    /// decide what to record from what everything recorded before it left. `admit` refuses it, as `refused` does there,
    /// or says what the person changes with it, as [`Admitted`] says.
    pub(super) async fn record_in_order(
        &self,
        origin: Origin,
        body: EventBody,
        raw: Option<String>,
        admit: impl FnOnce(&LiveState) -> Result<Admitted, ApiError>,
    ) -> Result<(SessionEvent, bool), ApiError> {
        let hub = self.hub()?;
        let run = origin.run();
        let (Step { body, mut change, next, waits }, still_current, admitted, mode_held) = {
            let state = self.state.lock();
            let admitted = admit(&state)?;
            // The current agent saying it, which only a running one can, or the hub with one running.
            let agent = state.agent.is_some() || run.is_some();
            let still_current = run.is_some() && state.agent.map(|agent| agent.run) == run;
            (state.progress.step(origin, body, agent), still_current, admitted, state.settings.mode)
        };
        if admitted.interaction.is_some() {
            change.interaction = admitted.interaction;
        }
        note_recorded(origin, &mut change);
        if origin.speaks_for_session() {
            if let EventBody::AgentInit { agent_session_id, .. } = &body {
                change.initialized = Some(crate::store::Initialized { conversation: agent_session_id.clone() });
            }
            // Only while its run is still the session's, as of now, in order with what's recorded: a run whose exit
            // was taken in since its caller looked has no claims on the record for its exit to take off. The mode it
            // says is the record's from then, which the next launch starts in, so a chat that was planning when its
            // agent went is still planning when it comes back.
            if still_current {
                change.claims = claims_of(run, &body);
                let said = mode_said(&body).filter(|said| *said != mode_held);
                change.settings = said.map(|mode| Settings { mode: Some(mode), ..Settings::default() });
            }
        }
        if admitted.settings.is_some() {
            change.settings = admitted.settings;
        }
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
        let (ready, turn_ended, between_turns, left, mode_changed) = {
            let mut state = self.state.lock();
            let was_ready = state.ready_for_queue();
            let (was_in_turn, was_on_a_turn) = (state.progress.status.in_turn(), state.on_a_turn());
            let (had, was) = (state.interaction.device().map(|had| had.id.clone()), state.interaction.mode());
            take_all(&mut state, recorded);
            if let Some(settings) = &made.settings {
                state.settings = state.settings.clone().changed(settings);
            }
            let turn_ended = was_in_turn && !state.progress.status.in_turn();
            let left = left_conversation(had.as_ref(), &state.interaction);
            let mode_changed = state.interaction.mode() != was;
            let ready = !was_ready && state.ready_for_queue();
            let after_the_turn = turn_ended && !state.after_the_turn.is_empty();
            (ready, after_the_turn, was_on_a_turn && !state.on_a_turn(), left, mode_changed)
        };
        if mode_changed {
            self.wake_the_mode();
        }
        // A prompt typed while the mode was on ended it, so what the device was reading out is settled.
        if left {
            self.settle_hearing(hub).await;
        }
        self.after_an_exit(hub, &appended).await;
        self.wake_between_turns(between_turns, turn_ended, ready);
        if made.waits {
            self.wait_for_pickup();
        }
        if made.changed {
            hub.work_changed();
            self.tell_cache_unless_working().await;
        }
        let by_the_person = appended.iter().any(|appended| appended.by_the_person);
        // A prompt sent while the agent works changes no status, but it does change when the person was last active. The
        // feed leaves out a copy whose revision didn't move, so one that took none costs only the read.
        if (made.changed || by_the_person || changed_beside_status(&appended))
            && let Err(e) = self.announce(hub).await
        {
            error!(
                name: "session.status_untold",
                session_id = %self.id,
                error = %e,
                "Couldn't announce a change of status"
            );
        }
        if by_the_person {
            hub.announce_workspace(&self.workspace_id).await;
        }
        for account in appended.iter().filter_map(|appended| appended.account.clone()) {
            hub.notify(HubNotice::Account(account));
        }
        self.tell_subagents(hub, &appended).await;
        if made.turn_ended {
            hub.read_pull_request_after_turn(&self.workspace_id);
            hub.read_lines_after_turn(&self.workspace_id);
        }
    }

    /// Asks the command task for what comes between turns, in the order it comes in: the tool servers to connect to
    /// again, if the agent is on no turn any more, then what was kept for the turn's end, if that `turn_ended`, and then
    /// what's queued, if it's `ready` for it.
    fn wake_between_turns(&self, on_none: bool, turn_ended: bool, ready: bool) {
        if on_none {
            self.wake_reconnects();
        }
        if turn_ended {
            self.wake_after_the_turn();
        }
        if ready {
            self.wake_queue();
        }
    }

    /// Tells everyone following the feed what the subagents `appended` changed are now.
    async fn tell_subagents(&self, hub: &Hub, appended: &[&Appended]) {
        let subagents: Vec<SubagentId> = appended.iter().flat_map(|appended| appended.subagents.clone()).collect();
        if !subagents.is_empty()
            && let Err(e) = self.announce_subagents(hub, subagents).await
        {
            error!(
                name: "session.subagent_untold",
                session_id = %self.id,
                error = %e,
                "Couldn't announce a change to a subagent"
            );
        }
    }

    /// Lets go of what each run whose exit `appended` has kept, as [`Hub::keeper_went`] says.
    async fn after_an_exit(&self, hub: &Hub, appended: &[&Appended]) {
        let exited = appended.iter().filter(|appended| matches!(appended.event.body, EventBody::AgentExited { .. }));
        for run in exited.filter_map(|appended| appended.event.run) {
            hub.keeper_went(Keeper::Run(run)).await;
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

    /// Tells everyone following the feed what the records of `changed` are now, read back from the store as the
    /// session is.
    async fn announce_subagents(&self, hub: &Hub, changed: Vec<SubagentId>) -> Result<(), ApiError> {
        let id = self.id.clone();
        let subagents = hub
            .blocking(move |store| {
                let records = changed.iter().map(|subagent| store.get_subagent(&id, subagent));
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
            error!(name: "session.event_unrecorded", session_id = %self.id, error = %e, "Couldn't record an event");
        }
    }
}

/// Whether `appended` changed the session's record beside its status. A question answered while another is open
/// changes no status, but it takes any answer being typed to it. What the agent waits on and has yet to take are on the
/// record too, and an event can change them with no change of status, as a prompt taken mid-turn does. A run's exit
/// takes its claims off the record whatever else it changes, as when its session had moved on.
fn changed_beside_status(appended: &[&Appended]) -> bool {
    appended.iter().any(|appended| {
        matches!(appended.event.body, EventBody::ApprovalResolved { .. })
            || appended.untaken.is_some()
            || appended.approvals.is_some()
            || appended.dequeued
            || appended.claims_taken_off.is_some()
    })
}

/// What a commit's steps made of the session, besides its log, for [`LiveSession::after_commit`] to tell of.
struct Made {
    /// Its record says something new of its status, or of its queue being held.
    changed: bool,
    /// The model, mode and effort the commit changed on its record, which the next launch starts with.
    settings: Option<Settings>,
    /// It waits for the agent to pick up again, as [`super::status::after_background`] says.
    waits: bool,
    turn_ended: bool,
}

impl Made {
    fn of(change: &SessionChange, waits: bool) -> Self {
        let turn_ended = change.status.as_ref().is_some_and(|status| matches!(status.turn, TurnEdge::Ends(_)));
        Self { changed: !change.is_none(), settings: change.settings.clone(), waits, turn_ended }
    }
}

/// What `body` says its run, `run`, claims, if it says, which the session holds that run to until it exits.
fn claims_of(run: Option<RunId>, body: &EventBody) -> Option<RunCapabilities> {
    run.zip(body.claims()).map(|(run, claims)| RunCapabilities::new(run, claims.clone()))
}

/// What the person changes with an event, as an `admit` passed to [`LiveSession::record_in_order`] says, which commits
/// with it.
#[derive(Default)]
pub(super) struct Admitted {
    /// How the person talks with the agent from the event on, when it changes that, as a prompt they type does.
    pub(super) interaction: Option<Interaction>,
    /// The model, mode and effort they changed, which the event says they did.
    pub(super) settings: Option<Settings>,
}

/// The approval mode `body` says the agent is in, if it's the agent saying it changed mode, or confirming a change.
fn mode_said(body: &EventBody) -> Option<ApprovalMode> {
    let EventBody::AgentMode { mode } = body else { return None };
    Some(*mode)
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
/// Takes in what one commit recorded, `recorded`, oldest first, and tells subscribers once for each run of events from
/// the agent or the same subagent, at the newest, as [`LiveSession::after_commit`] says.
fn take_all(state: &mut LiveState, recorded: Vec<(Origin, &Appended, Next)>) {
    let mut ended_others = false;
    let mut recorded = recorded.into_iter().peekable();
    while let Some((origin, appended, next)) = recorded.next() {
        ended_others |= take(state, origin, appended, next);
        let subagent = &appended.event.subagent;
        if recorded.peek().is_none_or(|(_, after, _)| after.event.subagent != *subagent) {
            let seq = appended.event.seq;
            publish(state, LiveItem::Appended { seq, subagent: subagent.clone(), ended_others });
            ended_others = false;
        }
    }
}

fn take(state: &mut LiveState, origin: Origin, appended: &Appended, next: Next) -> bool {
    let (seq, body, subagent) = (appended.event.seq, &appended.event.body, &appended.event.subagent);
    state.last_seq = seq;
    // The mode the event was recorded under is the record's, once the event changed it: on, from the rule that turns
    // it on, with the device that rule names, and off, from a prompt typed while it was on.
    match (body, appended.event.interaction) {
        (EventBody::ConversationStarted { device }, _) => {
            set_interaction(state, Interaction::Conversation { device: Box::new(device.clone()) });
        }
        (_, InteractionMode::Typing) => {
            set_interaction(state, Interaction::Typing);
        }
        (_, InteractionMode::Conversation | InteractionMode::Unknown) => {}
    }
    if origin.speaks_for_session() {
        if let EventBody::AgentInit { agent_session_id, .. } = body {
            state.agent_opened = true;
            if let Some(id) = agent_session_id {
                state.agent_session_id = Some(id.clone());
            }
        }
        if let Some(claims) =
            claims_of(origin.run(), body).filter(|claims| state.agent.is_some_and(|a| a.run == claims.run))
        {
            state.capabilities = Some(claims);
        }
    }
    let others = |tail: &Tail| tail.messages.iter().filter(|message| message.subagent != *subagent).count();
    let others_before = others(&state.tail);
    state.tail.on_event(seq, origin.run(), body);
    let ended_others = others(&state.tail) < others_before;
    state.progress.take(next, seq, appended.event.ts_ms, appended.untaken.clone());
    ended_others
}
