//! Changing the model, mode and effort a chat runs with.

use protocol::capability::settings::Settings;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::EventBody;
use protocol::model::Session;
use protocol::worker::Delivery;

use super::LiveSession;
use super::controls::Askable;
use crate::hub::Hub;

impl LiveSession {
    /// Saved only once a running agent has agreed, so what's shown is never ahead of what's true. A running agent reports
    /// a change of mode itself, and with none running, it goes in the log here, in the commit that saves it, so the log
    /// says when every change was made. *Unconfirmed:* that the hub records it.
    ///
    /// It's refused unless the chat's kind takes the settings the change leaves it with, which hold every setting the
    /// change names, so this is the only check a change needs. For a kind that discovers its models, those are judged
    /// by what the agent offers where the chat runs. They're judged under the lock that keeps changes in order, against
    /// the settings the change before left.
    pub(crate) async fn configure(&self, mut change: Settings) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        // Before the lock, which a launch takes, since the agent may be asked what it offers, which can take a while.
        let discovered =
            if hub.agent(self.agent)?.discovers_models() && (change.model.is_some() || change.effort.is_some()) {
                let target = protocol::client::admin::ModelTarget::Session(self.id.clone());
                Some(hub.agent_for(self.agent, target).await?)
            } else {
                None
            };
        let _one_at_a_time = self.configuring.lock().await;
        let current = self.state.lock().settings.clone();
        match discovered {
            Some(description) => {
                // Whoever asks, so a model the agent stopped offering can't be chosen as a full ID instead.
                if let Some(model) = &change.model
                    && description.label(model).is_none()
                {
                    return Err(ApiError::new(ErrorCode::InvalidRequest, "the agent doesn't offer that model here"));
                }
                if let Some(model) = &change.model
                    && change.effort.is_none()
                {
                    change.effort = Some(description.effort_for_choice(model, Some(&current.effort)));
                }
                description
                    .takes(&current.changed(&change).settings())
                    .map_err(|e| ApiError::new(ErrorCode::InvalidRequest, e.to_string()))?;
            }
            None => hub.offered(self.agent, &current.changed(&change).settings())?,
        }
        let mut agent_took_it = false;
        if let Some(link) = hub.existing_worker(&self.workspace_id) {
            let delivery = match self.askable()? {
                Askable::Claimed(claims) => {
                    self.control_claimed(&link, protocol::capability::Control::Change(change.clone()), &claims).await?
                }
                Askable::NotRunning => Delivery::AgentNotRunning,
            };
            agent_took_it = delivery == Delivery::Delivered;
        }
        let was = self.state.lock().settings.clone();
        match change.mode.filter(|mode| *mode != was.mode && !agent_took_it) {
            Some(mode) => self.record_settings(EventBody::AgentMode { mode }, change.clone()).await?,
            None => self.keep_settings(&hub, change.clone()).await?,
        }
        // Another model's cache doesn't hold the conversation, so the next request finds nothing cached.
        if change.model.is_some_and(|model| model != was.model) {
            self.forget_cache().await;
        }
        self.announce(&hub).await
    }

    /// Saves the model, mode and effort `change` sets, where no event says they changed, and launches the next agent
    /// with them.
    pub(super) async fn keep_settings(&self, hub: &Hub, change: Settings) -> Result<(), ApiError> {
        let (id, saved) = (self.id.clone(), change.clone());
        hub.blocking(move |store| store.set_session_settings(&id, &saved)).await?;
        let mut state = self.state.lock();
        state.settings = state.settings.clone().changed(&change);
        Ok(())
    }
}
