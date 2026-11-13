//! What each kind of agent the hub runs is called and offers, as its adapter describes it, which the hub tells clients
//! and gives each new chat what it leaves out from.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AgentDescription, AgentFeature, AgentKind, SessionSettingsChange};

use super::Hub;

impl Hub {
    /// Every kind's description, which doesn't change while the hub runs.
    pub(crate) fn agent_list(&self) -> Vec<AgentDescription> {
        self.agents.clone()
    }

    /// The kind a chat runs when nothing names one, such as the first chat of a workspace made from the new-workspace
    /// dialog: the first described, as client-core picks for a new chat, or Claude, which a hub that describes none
    /// refuses as it would any kind.
    pub(super) fn new_chat_kind(&self) -> AgentKind {
        self.agents.first().map_or(AgentKind::Claude, |agent| agent.kind)
    }

    /// `kind`'s description, or a refusal for a kind this hub doesn't run.
    pub(super) fn agent(&self, kind: AgentKind) -> Result<&AgentDescription, ApiError> {
        let described = self.agents.iter().find(|agent| agent.kind == kind);
        described.ok_or_else(|| ApiError::new(ErrorCode::InvalidRequest, "this hub doesn't run that kind of agent"))
    }

    /// Refuses what a chat running `kind` can't be given, as [`AgentDescription::takes`] says.
    pub(super) fn offered(&self, kind: AgentKind, change: &SessionSettingsChange) -> Result<(), ApiError> {
        let not_offered = |e: protocol::model::NotOffered| ApiError::new(ErrorCode::InvalidRequest, e.to_string());
        self.agent(kind)?.takes(change).map_err(not_offered)
    }

    /// Refuses a compaction of an agent that doesn't compact when asked.
    pub(super) fn compacts(&self, kind: AgentKind) -> Result<(), ApiError> {
        if self.agent(kind)?.has(AgentFeature::Compacts) {
            Ok(())
        } else {
            Err(ApiError::new(ErrorCode::InvalidRequest, "this agent doesn't compact when asked"))
        }
    }
}

#[cfg(test)]
mod tests;
