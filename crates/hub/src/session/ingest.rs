//! Taking in what the agent sends.

use std::sync::Arc;

use protocol::attachment::NewAttachment;
use protocol::capability::approvals::{ApprovalMode, Asks, Decision, Remember, Request};
use protocol::capability::settings::Settings;
use protocol::event::{CompactionOutcome, EventBody, ExitReason, SessionEvent};
use protocol::model::{ApprovalId, RunId};
use protocol::tail::{Retrying, StreamDelta, Tail};
use tokio::sync::mpsc;
use tracing::{error, warn};

use super::{Item, LiveItem, LiveSession, Origin, publish};
use crate::store::cancelled_approval;

pub(crate) async fn run_ingest(session: Arc<LiveSession>, mut items: mpsc::Receiver<Item>) {
    while let Some(item) = items.recv().await {
        match item {
            Item::Event { run, seq, .. } | Item::Report { run, seq, .. } if session.recorded_already(run, seq) => {}
            Item::Event { run, body, raw, attachments, seq } => {
                ingest_event(&session, run, *body, raw, attachments, seq).await;
            }
            Item::Delta { run, delta } => session.apply_delta(run, delta),
            Item::Tail { run, tail } => session.take_tail(run, tail),
            Item::Retrying { run, retrying } => session.retrying(run, retrying),
            Item::Report { run, report, .. } => session.take_report(run, report).await,
            Item::AgentLost { link, reason } => session.agent_lost(link, reason).await,
            Item::PickupDue { wait } => session.no_pickup(wait).await,
            Item::Flush(done) => {
                let _ = done.send(());
            }
            Item::End => break,
        }
    }
}

async fn ingest_event(
    session: &Arc<LiveSession>,
    run: RunId,
    body: EventBody,
    raw: Option<String>,
    attachments: Vec<NewAttachment>,
    numbered: Option<u64>,
) {
    if !attachments.is_empty()
        && let Ok(hub) = session.hub()
    {
        hub.keep_from_agent(&session.id, attachments).await;
    }
    let current = session.state.lock().agent.is_some_and(|agent| agent.run == run);
    follow(session, run, current, &body).await;
    let body = session.with_waits(body).await;
    let origin = Origin::Run { run, current, numbered };
    if current && let Some(approval_id) = allowed_by_the_hub(session, &body) {
        allow(session, origin, (body, raw), approval_id).await;
        return;
    }
    let stale_approval = if let EventBody::ApprovalRequested { approval_id, .. } = &body
        && !current
    {
        Some(approval_id.clone())
    } else {
        None
    };
    match session.record(origin, body, raw).await {
        // Heard when the hub recorded it, by the hub's clock, as everything else in the log is timed.
        Ok((SessionEvent { body: EventBody::AgentLimits { limits }, ts_ms, .. }, _)) => {
            // What the agent hears by itself says nothing of the limits it leaves out.
            session.heard_limits(run, limits, false, ts_ms);
        }
        Ok(_) => {}
        Err(e) => error!(session = %session.id, "could not record agent output: {e}"),
    }
    if let Some(approval_id) = stale_approval {
        session.record_or_log(Origin::Hub, cancelled_approval(approval_id)).await;
    }
}

/// The ID of the request in `body`, if the hub allows it itself, as [`the_hub_allows`] says.
fn allowed_by_the_hub(session: &LiveSession, body: &EventBody) -> Option<ApprovalId> {
    let EventBody::ApprovalRequested { approval_id, request, .. } = body else { return None };
    let mode = session.state.lock().settings.mode;
    let set_to = session.hub().is_ok_and(|hub| hub.allows_asked_in_full_access());
    the_hub_allows(mode, request, set_to).then(|| approval_id.clone())
}

/// Whether the hub allows `request` itself, in a session in `mode`, when its settings are `set_to`: a call asked about
/// with full access, which comes whatever the mode, as Claude's CLI asks before a command it can't check. Anything
/// else is the person's to decide whatever the settings say, and so is anything asked in plan mode. The hub decides
/// this, and not an agent's adapter, since every agent's requests reach the hub and the setting is the hub's.
pub(super) fn the_hub_allows(mode: ApprovalMode, request: &Request, set_to: bool) -> bool {
    set_to && mode.is_full_access() && matches!(request.asks, Asks::Call { .. })
}

/// Records the request `asked`, which is `approval_id`, with the hub's decision allowing it, and passes that on to the
/// agent. It's passed on from a task of its own, since the agent's worker replies on the connection whose events this
/// task takes in, and waits for this task when there are many, so waiting for the reply here could leave each waiting
/// on the other.
async fn allow(
    session: &Arc<LiveSession>,
    origin: Origin,
    asked: (EventBody, Option<String>),
    approval_id: ApprovalId,
) {
    let allowed = Decision::Allow { remember: Remember::Once };
    let answer = EventBody::ApprovalResolved {
        approval_id: approval_id.clone(),
        decision: Some(Box::new(allowed.clone())),
        by_setting: true,
    };
    if let Err(e) = session.record_answered(origin, asked, answer).await {
        error!(session = %session.id, "could not record a tool's use the hub allowed: {e}");
        return;
    }
    let session = session.clone();
    tokio::spawn(async move {
        if let Err(e) = session.pass_on(approval_id, allowed).await {
            warn!(session = %session.id, "could not pass on a tool's use the hub allowed: {e}");
        }
    });
}

/// What the session does about an event from the agent of `run` before it's recorded, `current` if that's the agent
/// it has now.
async fn follow(session: &Arc<LiveSession>, run: RunId, current: bool, body: &EventBody) {
    match body {
        EventBody::AgentInit { .. } => {
            if current {
                session.launched(run);
            }
        }
        EventBody::AgentMode { mode } if current => session.note_mode(*mode).await,
        // What the cache held is no longer the conversation, and the next request writes the summary.
        EventBody::AgentCompacted { outcome: CompactionOutcome::Compacted { .. }, .. } if current => {
            session.announce_cache(None).await;
        }
        EventBody::AgentExited { .. } => {
            if current {
                session.state.lock().agent = None;
            }
            session.cancel_approvals(Some(run)).await;
        }
        EventBody::UserPrompt { .. }
        | EventBody::CompactionAsked { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::OwnTurn(_)
        | EventBody::AgentLimits { .. }
        | EventBody::Assistant { .. }
        | EventBody::AgentUser { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::ApprovalRequested { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::Notice { .. }
        | EventBody::RequestFailed { .. }
        | EventBody::AgentMode { .. }
        | EventBody::SubagentStarted { .. }
        | EventBody::SubagentContext { .. }
        | EventBody::SubagentEnded { .. }
        | EventBody::TaskChanged { .. }
        | EventBody::AgentOther { .. }
        | EventBody::AgentCompacting { .. }
        | EventBody::AgentCompacted { .. }
        | EventBody::CacheExpired { .. }
        | EventBody::ConversationStarted { .. }
        | EventBody::Unknown => {}
    }
}

impl LiveSession {
    /// Only the current run's live output reaches anyone. A replaced agent's is dropped here, so it
    /// can't appear beside its replacement's.
    fn apply_delta(&self, run: RunId, mut delta: StreamDelta) {
        let mut state = self.state.lock();
        if state.agent.is_none_or(|agent| agent.run != run) {
            return;
        }
        delta.run = Some(run);
        state.tail.apply(&delta);
        publish(&mut state, LiveItem::Delta(delta));
    }

    /// Takes what the agent of `run` is writing from what its worker kept of it, in place of what this hub heard. Only
    /// the current run's reaches anyone, as with its deltas.
    fn take_tail(&self, run: RunId, mut tail: Tail) {
        let mut state = self.state.lock();
        if state.agent.is_none_or(|agent| agent.run != run) {
            return;
        }
        // The worker keeps the line said for the agent until its adapter closes the wait, which it may not have yet
        // after the mode ended, but nobody is listening for it.
        if state.interaction.is_typing() {
            tail.covering = None;
        }
        state.tail.replace_run(run, tail);
        publish(&mut state, LiveItem::Rewritten);
    }

    /// The agent of `run` is trying a failed request again. Only the current run's reaches anyone, as with its deltas,
    /// and each subscriber is sent the tail whole, since no delta says it.
    fn retrying(&self, run: RunId, retrying: Retrying) {
        let mut state = self.state.lock();
        if state.agent.is_none_or(|agent| agent.run != run) {
            return;
        }
        state.tail.retrying(run, retrying);
        publish(&mut state, LiveItem::Rewritten);
    }

    /// The agent changed mode, or confirmed a change. The next launch has to start in this one, so a chat that was
    /// planning when its agent went is still planning when it comes back.
    async fn note_mode(&self, mode: ApprovalMode) {
        let Ok(hub) = self.hub() else { return };
        if self.state.lock().settings.mode == mode {
            return;
        }
        let newer = Settings { mode: Some(mode), ..Settings::default() };
        if let Err(e) = self.keep_settings(&hub, newer).await {
            error!(session = %self.id, "could not record the agent's mode: {e}");
            return;
        }
        if let Err(e) = self.announce(&hub).await {
            error!(session = %self.id, "could not announce the agent's mode: {e}");
        }
    }

    /// Closes open approvals that nobody can answer now: those from `run`, or all if it's `None`.
    pub(crate) async fn cancel_approvals(&self, run: Option<RunId>) {
        let open: Vec<ApprovalId> = {
            let state = self.state.lock();
            let from_this_run = |asked_by: Option<RunId>| run.is_none_or(|run| asked_by.is_none_or(|by| by == run));
            state.progress.approvals.iter().filter(|(_, by)| from_this_run(**by)).map(|(id, _)| id.clone()).collect()
        };
        for approval_id in open {
            self.record_or_log(Origin::Hub, cancelled_approval(approval_id)).await;
        }
    }

    /// Whether the session has recorded this already, as [`heard_before`] says.
    fn recorded_already(&self, run: RunId, seq: Option<u64>) -> bool {
        seq.is_some_and(|seq| heard_before(&mut self.state.lock().recorded, run, seq))
    }

    /// Reports that the agent is gone, unless a newer one has already replaced it.
    pub(crate) async fn agent_lost(&self, link: u64, reason: ExitReason) {
        let lost = {
            let mut state = self.state.lock();
            let lost = state.agent.filter(|agent| agent.link == link);
            if lost.is_some() {
                state.agent = None;
            }
            lost
        };
        if let Some(lost) = lost {
            self.cancel_approvals(None).await;
            // Its agent went with its worker, so it can't say it dropped what it hadn't taken.
            let untaken = self.state.lock().progress.queue.untaken().to_vec();
            for prompt in untaken {
                self.record_or_log(Origin::Hub, EventBody::PromptDropped { prompt }).await;
            }
            let body = EventBody::AgentExited { reason, code: None, signal: None, stderr_tail: vec![] };
            self.record_or_log(Origin::Run { run: lost.run, current: true, numbered: None }, body).await;
        }
    }

    /// Lets go of its agent if it's on the worker `link`, which the hub is about to stop with nothing at work, to
    /// replace it. The agent's exit is then of one the session has moved on from, so it changes nothing, even if a
    /// prompt is recorded first, and the next prompt starts an agent on the next worker, which resumes the
    /// conversation.
    pub(crate) fn let_go_of_agent_on(&self, link: u64) {
        let mut state = self.state.lock();
        if state.agent.is_some_and(|agent| agent.link == link) {
            state.agent = None;
        }
    }
}

/// Whether what the worker numbered `seq`, from the agent `run`, is at or before `recorded`, the last the session took
/// in, which a worker sends again to a hub that connects to it until a hub says it has recorded it. Otherwise it's the
/// last from then on, since a worker numbers what it sends in order, and the session takes it in in that order. What
/// another run sent is never the same.
pub(super) fn heard_before(recorded: &mut Option<(RunId, u64)>, run: RunId, seq: u64) -> bool {
    if recorded.is_some_and(|(recorded_run, through)| recorded_run == run && seq <= through) {
        return true;
    }
    *recorded = Some((run, seq));
    false
}
