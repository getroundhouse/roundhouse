//! A chat's waits on its pull request, as the person calls one off, and as they leave its list of work in the
//! background with no agent running to take them off it.

use protocol::client::ApiError;
use protocol::event::{EventBody, TaskKind};
use protocol::model::{Session, WaitId};
use tokio::sync::MutexGuard;
use tracing::info;

use super::{LiveSession, Origin};
use crate::store::now_ms;

impl LiveSession {
    /// Calls off the chat's wait `wait`, for the person, if it's the one the chat has open. A running CLI hears as it next
    /// asks after its task, as often as the worker says to, takes the task off its list, and takes a turn to tell the
    /// agent. With no agent running, its waits are taken off the list here, as [`LiveSession::unlist_waits`] says, and
    /// the agent hears once it next starts, since its CLI lists the task again then and asks after it. An agent being
    /// started meanwhile either finishes first, and its CLI hears, or starts after, and its CLI hears as it asks after
    /// the task it lists again.
    pub(crate) async fn call_off_wait(&self, wait: WaitId) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        {
            let starts = self.configuring.lock().await;
            let (id, at_ms) = (self.id.clone(), now_ms());
            if let Some(wait) = hub.blocking(move |store| store.call_off_session_wait(&id, &wait, at_ms)).await? {
                info!(%wait, session = %self.id, "the person called off a wait");
            }
            self.unlist_waits_holding(&starts).await?;
        }
        self.announce(&hub).await
    }

    /// Takes the chat's waits off its list of work in the background, with no agent running to once they're over, as
    /// after the hub started again with the chat at work only on its wait, and then something ended it, such as
    /// closing the chat or archiving its workspace, or a read ended it with nobody to hear until the next prompt. That
    /// leaves the chat idle if they were all it had there, rather than at work, keeping the machine awake, until the
    /// next hub starts. Not while the hub has one open, which they may be the CLI's copy of, since the list has its
    /// own IDs for them.
    pub(crate) async fn unlist_waits(&self) -> Result<(), ApiError> {
        let starts = self.configuring.lock().await;
        self.unlist_waits_holding(&starts).await
    }

    /// As [`LiveSession::unlist_waits`] does, holding `configuring`, so no agent starts meanwhile.
    pub(super) async fn unlist_waits_holding(&self, _starts: &MutexGuard<'_, ()>) -> Result<(), ApiError> {
        let listed = self.state.lock().background_tasks.clone();
        let left: Vec<_> = listed.iter().filter(|task| task.kind != TaskKind::Wait).cloned().collect();
        if self.agent_running() || left.len() == listed.len() {
            return Ok(());
        }
        let (hub, id) = (self.hub()?, self.id.clone());
        if hub.blocking(move |store| store.get_session(&id)).await?.session.wait.is_none() {
            self.record(Origin::Hub, EventBody::AgentTasks { tasks: left, stopped: Vec::new() }, None).await?;
        }
        Ok(())
    }
}
