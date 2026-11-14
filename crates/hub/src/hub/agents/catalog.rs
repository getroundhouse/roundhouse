use super::super::Hub;
use protocol::capability::settings::Settings;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AgentDescription, AgentFeature, AgentKind, SessionSettings, SessionSettingsChange};

impl Hub {
    /// Every kind's description, which doesn't change while the hub runs.
    pub(crate) fn agent_list(&self) -> Vec<AgentDescription> {
        self.agents.clone()
    }

    /// The kind a chat runs when nothing names one, such as the first chat of a workspace made from the new-workspace
    /// dialog: the first described, as client-core picks for a new chat, or Claude, which a hub that describes none
    /// refuses as it would any kind.
    pub(in super::super) fn new_chat_kind(&self) -> AgentKind {
        self.agents.first().map_or(AgentKind::Claude, |agent| agent.kind)
    }

    /// `kind`'s description, or a refusal for a kind this hub doesn't run.
    pub(in super::super) fn agent(&self, kind: AgentKind) -> Result<&AgentDescription, ApiError> {
        let described = self.agents.iter().find(|agent| agent.kind == kind);
        described.ok_or_else(|| ApiError::new(ErrorCode::InvalidRequest, "this hub doesn't run that kind of agent"))
    }

    /// Refuses what a chat running `kind` can't be given, as [`AgentDescription::takes`] says.
    pub(crate) fn offered(&self, kind: AgentKind, change: &SessionSettingsChange) -> Result<(), ApiError> {
        let not_offered = |e: protocol::model::NotOffered| ApiError::new(ErrorCode::InvalidRequest, e.to_string());
        self.agent(kind)?.takes(change).map_err(not_offered)
    }

    /// `settings`, as the hub keeps a chat's, as a launch of `kind` names them: by the modes its kind claims.
    pub(crate) fn launch_settings(&self, kind: AgentKind, settings: &SessionSettings) -> Result<Settings, ApiError> {
        let claims = self.agent(kind)?.capabilities.as_ref().ok_or_else(|| {
            ApiError::new(ErrorCode::InvalidRequest, "this hub doesn't know what that kind of agent can do")
        })?;
        settings.for_capabilities(claims).map_err(|e| ApiError::new(ErrorCode::InvalidRequest, e.to_string()))
    }

    /// Refuses a compaction of an agent that doesn't compact when asked.
    pub(in super::super) fn compacts(&self, kind: AgentKind) -> Result<(), ApiError> {
        if self.agent(kind)?.has(AgentFeature::Compacts) {
            Ok(())
        } else {
            Err(ApiError::new(ErrorCode::InvalidRequest, "this agent doesn't compact when asked"))
        }
    }
}
