//! Writing an event to the log and telling everyone who's watching.

use protocol::client::{ApiError, HubNotice};
use protocol::event::{AgentTask, EventBody, SessionEvent};
use protocol::model::{ClientMessageId, Session, SessionStatus};
use tracing::{error, warn};

use super::status::{ends_turn, next_status, open_after};
use super::{LiveItem, LiveSession, LiveState, Origin, publish};
use crate::hub::Hub;
use crate::store::StatusChange;

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

/// The status and the work in the background once `body` is applied, and what of that has changed, to commit with it.
/// Only the hub and the current agent speak for the session.
fn after(state: &LiveState, origin: Origin, body: &EventBody) -> (SessionStatus, Option<StatusChange>, Vec<AgentTask>) {
    if !origin.speaks_for_session() {
        return (state.status, None, state.background_tasks.clone());
    }
    let (open, tasks) = open_after(&state.pending_approvals, &state.background_tasks, body);
    let next = next_status(state.status, body, open);
    let ends_turn = ends_turn(state.status, next);
    let background_tasks = (tasks != state.background_tasks).then(|| tasks.clone());
    let changed = next != state.status || background_tasks.is_some();
    (next, changed.then_some(StatusChange { status: next, ends_turn, background_tasks }), tasks)
}

impl LiveSession {
    /// Commits to the log first, so nothing is seen that isn't durable, then updates live state.
    /// One at a time: both tasks record, and status has to change in the same order as the log.
    pub(crate) async fn record(
        &self,
        origin: Origin,
        body: EventBody,
        raw: Option<String>,
        client_msg_id: Option<ClientMessageId>,
    ) -> Result<(SessionEvent, bool), ApiError> {
        let _in_order = self.recording.lock().await;
        let hub = self.hub()?;
        let run = origin.run();
        let (next, change, tasks) = after(&self.state.lock(), origin, &body);
        let announce = change.is_some();
        let id = self.id.clone();
        let for_store = body.clone();
        let appended = hub
            .blocking(move |store| store.append(&id, run, &for_store, raw.as_deref(), client_msg_id.as_ref(), change))
            .await?;
        if appended.duplicate {
            return Ok((appended.event, true));
        }
        let seq = appended.event.seq;
        {
            let mut state = self.state.lock();
            state.last_seq = seq;
            state.tail.on_event(run, &body);
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
            publish(&mut state, LiveItem::Appended(seq));
        }
        if announce && let Err(e) = self.announce(&hub).await {
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

    /// For where nobody is waiting to hear that recording failed.
    pub(crate) async fn record_or_log(&self, origin: Origin, body: EventBody) {
        if let Err(e) = self.record(origin, body, None, None).await {
            error!(session = %self.id, "could not record an event: {e}");
        }
    }
}
