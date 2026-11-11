//! Taking in what the agent sends.

use std::sync::Arc;

use protocol::attachment::NewAttachment;
use protocol::event::{EventBody, ExitReason, SessionEvent};
use protocol::model::{ApprovalId, ContextUsage, RunId, SessionSettingsChange};
use protocol::tail::StreamDelta;
use tokio::sync::mpsc;
use tracing::error;

use super::{Item, LiveItem, LiveSession, Origin, publish};
use crate::store::cancelled_approval;

pub(crate) async fn run_ingest(session: Arc<LiveSession>, mut items: mpsc::Receiver<Item>) {
    while let Some(item) = items.recv().await {
        match item {
            Item::Event { run, body, raw, attachments } => ingest_event(&session, run, body, raw, attachments).await,
            Item::Delta { run, delta } => session.apply_delta(run, delta),
            Item::Context { run, context } => session.note_context(run, context).await,
            // Heard when the hub took it, by the hub's clock, as an event the agent said it in is timed.
            Item::Limits { run, limits } => session.heard_limits(run, limits, crate::store::now_ms()),
            Item::WorkerLost(link) => session.agent_lost(link, ExitReason::WorkerLost).await,
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
) {
    if !attachments.is_empty()
        && let Ok(hub) = session.hub()
    {
        hub.keep_from_agent(&session.id, attachments).await;
    }
    let current = session.state.lock().agent.is_some_and(|agent| agent.run == run);
    match &body {
        EventBody::AgentInit { .. } => {
            if current {
                session.launched(run);
            }
            mark_opened(session).await;
        }
        EventBody::AgentMode { permission_mode } if current => session.note_mode(permission_mode.clone()).await,
        EventBody::AgentExited { .. } => {
            if current {
                session.state.lock().agent = None;
            }
            session.cancel_approvals(Some(run)).await;
        }
        EventBody::UserPrompt { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::AgentLimits { .. }
        | EventBody::Assistant { .. }
        | EventBody::AgentUser { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::ApprovalRequested { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentMode { .. }
        | EventBody::SubagentStarted { .. }
        | EventBody::SubagentEnded { .. }
        | EventBody::AgentOther { .. }
        | EventBody::Unknown => {}
    }
    let stale_approval = if let EventBody::ApprovalRequested { approval_id, .. } = &body
        && !current
    {
        Some(approval_id.clone())
    } else {
        None
    };
    match session.record(Origin::Run { run, current }, body, raw, None).await {
        // Heard when the hub recorded it, by the hub's clock, as everything else in the log is timed.
        Ok((SessionEvent { body: EventBody::AgentLimits { limits }, ts_ms, .. }, _)) => {
            session.heard_limits(run, limits, ts_ms);
        }
        Ok(_) => {}
        Err(e) => error!(session = %session.id, "could not record agent output: {e}"),
    }
    if let Some(approval_id) = stale_approval {
        session.record_or_log(Origin::Hub, cancelled_approval(approval_id)).await;
    }
}

/// Whether what `run` said of its context changes what the session's record says. A run that isn't the one going is
/// one that's been replaced or has exited, and what it says is of a conversation the session has moved on from.
pub(super) fn says_something_new(
    current: Option<RunId>,
    known: Option<&ContextUsage>,
    run: RunId,
    said: &ContextUsage,
) -> bool {
    current == Some(run) && known != Some(said)
}

async fn mark_opened(session: &Arc<LiveSession>) {
    let first_time = {
        let mut state = session.state.lock();
        !std::mem::replace(&mut state.agent_opened, true)
    };
    if !first_time {
        return;
    }
    let Ok(hub) = session.hub() else { return };
    let id = session.id.clone();
    if let Err(e) = hub.blocking(move |store| store.set_agent_opened(&id)).await {
        error!(session = %session.id, "could not record that the agent opened: {e}");
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

    /// The agent said how full its context is. Only the current run speaks for the session, and only a change is kept
    /// and announced, so the feed isn't told what it knows.
    async fn note_context(&self, run: RunId, context: ContextUsage) {
        let Ok(hub) = self.hub() else { return };
        {
            let state = self.state.lock();
            let current = state.agent.map(|agent| agent.run);
            if !says_something_new(current, state.context.as_ref(), run, &context) {
                return;
            }
        }
        let (id, kept) = (self.id.clone(), context.clone());
        if let Err(e) = hub.blocking(move |store| store.set_session_context(&id, &kept)).await {
            error!(session = %self.id, "could not record how full the agent's context is: {e}");
            return;
        }
        self.state.lock().context = Some(context);
        if let Err(e) = self.announce(&hub).await {
            error!(session = %self.id, "could not announce how full the agent's context is: {e}");
        }
    }

    /// The agent changed mode, or confirmed a change. The next launch has to start in this one.
    async fn note_mode(&self, mode: String) {
        let Ok(hub) = self.hub() else { return };
        if self.state.lock().settings.permission_mode.as_deref() == Some(mode.as_str()) {
            return;
        }
        let newer = SessionSettingsChange { permission_mode: Some(mode), ..SessionSettingsChange::default() };
        if let Err(e) = self.keep_options(&hub, newer).await {
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
            state.pending_approvals.iter().filter(|(_, by)| from_this_run(**by)).map(|(id, _)| id.clone()).collect()
        };
        for approval_id in open {
            self.record_or_log(Origin::Hub, cancelled_approval(approval_id)).await;
        }
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
            let untaken = self.state.lock().queue.untaken().to_vec();
            for prompt in untaken {
                self.record_or_log(Origin::Hub, EventBody::PromptDropped { prompt }).await;
            }
            let body = EventBody::AgentExited { reason, code: None, signal: None, stderr_tail: vec![] };
            self.record_or_log(Origin::Run { run: lost.run, current: true }, body).await;
        }
    }
}
