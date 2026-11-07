//! Taking in what the agent sends.

use std::sync::Arc;

use protocol::event::{EventBody, ExitReason};
use protocol::tail::StreamDelta;
use tokio::sync::mpsc;
use tracing::error;

use super::{Item, LiveItem, LiveSession, NO_RUN, publish};
use crate::store::cancelled_approval;

pub(crate) async fn run_ingest(session: Arc<LiveSession>, mut items: mpsc::Receiver<Item>) {
    while let Some(item) = items.recv().await {
        match item {
            Item::Event { run, body, raw } => ingest_event(&session, run, body, raw).await,
            Item::Delta(delta) => session.apply_delta(delta),
            Item::WorkerLost(link) => session.agent_lost(link, ExitReason::WorkerLost).await,
            Item::Flush(done) => {
                let _ = done.send(());
            }
        }
    }
}

async fn ingest_event(session: &Arc<LiveSession>, run: u64, body: EventBody, raw: Option<String>) {
    let current = session.state.lock().agent.is_some_and(|agent| agent.run == run);
    match &body {
        EventBody::AgentInit { .. } => mark_opened(session).await,
        EventBody::AgentMode { permission_mode } if current => session.note_mode(permission_mode.clone()).await,
        EventBody::AgentExited { .. } => {
            if current {
                session.state.lock().agent = None;
            }
            session.cancel_approvals(Some(run)).await;
        }
        EventBody::UserPrompt { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentState { .. }
        | EventBody::Assistant { .. }
        | EventBody::AgentUser { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::ApprovalRequested { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentMode { .. }
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
    if let Err(e) = session.record(body, raw, None, current, run).await {
        error!(session = session.id, "could not record agent output: {e}");
    }
    if let Some(approval_id) = stale_approval {
        session.record_or_log(cancelled_approval(approval_id)).await;
    }
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
        error!(session = session.id, "could not record that the agent opened: {e}");
    }
}

impl LiveSession {
    fn apply_delta(&self, delta: StreamDelta) {
        let mut state = self.state.lock();
        state.tail.apply(&delta);
        publish(&mut state, LiveItem::Delta(delta));
    }

    /// The agent changed mode, or confirmed a change. The next launch has to start in this one.
    async fn note_mode(&self, mode: String) {
        let Ok(hub) = self.hub() else { return };
        if self.state.lock().permission_mode.as_deref() == Some(mode.as_str()) {
            return;
        }
        let (id, saved) = (self.id.clone(), mode.clone());
        if let Err(e) = hub.blocking(move |store| store.set_session_options(&id, None, Some(&saved))).await {
            error!(session = self.id, "could not record the agent's mode: {e}");
            return;
        }
        self.state.lock().permission_mode = Some(mode);
        self.announce(&hub);
    }

    /// Closes open approvals that nobody can answer now: those from `run`, or all if it's `None`.
    pub(crate) async fn cancel_approvals(&self, run: Option<u64>) {
        let open: Vec<String> = {
            let state = self.state.lock();
            let from_this_run = |asked_by: u64| run.is_none_or(|run| asked_by == run || asked_by == NO_RUN);
            state.pending_approvals.iter().filter(|(_, by)| from_this_run(**by)).map(|(id, _)| id.clone()).collect()
        };
        for approval_id in open {
            self.record_or_log(cancelled_approval(approval_id)).await;
        }
    }

    /// Reports that the agent is gone, unless a newer one has already replaced it.
    pub(crate) async fn agent_lost(&self, link: u64, reason: ExitReason) {
        let was_running = {
            let mut state = self.state.lock();
            let matches = state.agent.is_some_and(|agent| agent.link == link);
            if matches {
                state.agent = None;
            }
            matches
        };
        if was_running {
            self.cancel_approvals(None).await;
            self.record_or_log(EventBody::AgentExited { reason, code: None, signal: None, stderr_tail: vec![] }).await;
        }
    }
}
