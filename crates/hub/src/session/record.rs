//! Writing an event to the log and telling everyone who's watching.

use protocol::client::{ApiError, HubNotice};
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ClientMessageId, Seq, Session, SessionStatus};
use tracing::{error, warn};

use super::status::{approvals_after, next_status};
use super::{LiveItem, LiveSession, NO_RUN, publish};
use crate::hub::Hub;
use crate::store::{self};

/// The first line of a prompt, cut at a word if it's long.
pub(crate) fn title_from_prompt(prompt: &str) -> Option<String> {
    const MOST: usize = 48;
    let line = prompt.lines().map(str::trim).find(|line| !line.is_empty())?;
    if line.chars().count() <= MOST {
        return Some(line.to_string());
    }
    let cut: String = line.chars().take(MOST).collect();
    let shortened = cut.rsplit_once(' ').map_or(cut.as_str(), |(start, _)| start);
    Some(format!("{}…", shortened.trim_end()))
}

impl LiveSession {
    /// Commits to the log first, so nothing is seen that isn't durable, then updates live state.
    /// One at a time: both tasks record, and status has to change in the same order as the log.
    /// `speaks_for_session` is false for a leftover event from a replaced agent, which mustn't change
    /// the status of the session its replacement is now running.
    pub(crate) async fn record(
        &self,
        body: EventBody,
        raw: Option<String>,
        client_msg_id: Option<ClientMessageId>,
        speaks_for_session: bool,
        run: u64,
    ) -> Result<(SessionEvent, bool), ApiError> {
        let _in_order = self.recording.lock().await;
        let hub = self.hub()?;
        let (next, new_status) = {
            let state = self.state.lock();
            if speaks_for_session {
                let next = next_status(state.status, &body, approvals_after(&state.pending_approvals, &body));
                (next, (next != state.status).then_some(next))
            } else {
                (state.status, None)
            }
        };
        let id = self.id.clone();
        let for_store = body.clone();
        let appended = hub
            .blocking(move |store| store.append(&id, &for_store, raw.as_deref(), client_msg_id.as_ref(), new_status))
            .await?;
        if appended.duplicate {
            return Ok((appended.event, true));
        }
        let seq = appended.event.seq;
        let mut state = self.state.lock();
        state.last_seq = seq;
        state.tail.on_event(&body);
        match &body {
            EventBody::ApprovalRequested { approval_id, .. } => {
                state.pending_approvals.insert(approval_id.clone(), run);
            }
            EventBody::ApprovalResolved { approval_id, .. } => {
                state.pending_approvals.remove(approval_id);
            }
            EventBody::UserPrompt { .. }
            | EventBody::AgentStarted { .. }
            | EventBody::AgentInit { .. }
            | EventBody::AgentState { .. }
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
        publish(&mut state, LiveItem::Appended(seq));
        drop(state);
        if new_status.is_some() {
            hub.notify(HubNotice::Session(self.record_now(next, seq, appended.event.ts_ms)));
        }
        Ok((appended.event, false))
    }

    fn record_now(&self, status: SessionStatus, last_seq: Seq, updated_at_ms: i64) -> Session {
        let state = self.state.lock();
        Session {
            id: self.id.clone(),
            workspace_id: self.workspace_id.clone(),
            agent: self.agent,
            model: state.model.clone(),
            permission_mode: state.permission_mode.clone(),
            status,
            last_seq,
            created_at_ms: self.created_at_ms,
            updated_at_ms,
            title: state.title.clone(),
            closed_at_ms: state.closed_at_ms,
        }
    }

    /// What this session is right now, for anything that changed outside the log.
    pub(crate) fn announce(&self, hub: &Hub) -> Session {
        let (status, last_seq) = {
            let state = self.state.lock();
            (state.status, state.last_seq)
        };
        let session = self.record_now(status, last_seq, store::now_ms());
        hub.notify(HubNotice::Session(session.clone()));
        session
    }

    /// The first prompt names the session.
    pub(crate) async fn name_from(&self, prompt: &str) {
        let Some(title) = title_from_prompt(prompt) else { return };
        if self.state.lock().title.is_some() {
            return;
        }
        let Ok(hub) = self.hub() else { return };
        let (id, saved) = (self.id.clone(), title.clone());
        if let Err(e) = hub.blocking(move |store| store.set_session_title(&id, &saved)).await {
            warn!(session = %self.id, "could not save the session's title: {e}");
        }
        self.state.lock().title = Some(title);
    }

    /// Records something the hub itself knows, as opposed to agent output.
    pub(crate) async fn record_own(&self, body: EventBody) -> Result<(SessionEvent, bool), ApiError> {
        self.record(body, None, None, true, NO_RUN).await
    }

    pub(crate) async fn record_or_log(&self, body: EventBody) {
        if let Err(e) = self.record_own(body).await {
            error!(session = %self.id, "could not record an event: {e}");
        }
    }
}
