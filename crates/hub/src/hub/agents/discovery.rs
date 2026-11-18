//! Which list of models applies where a chat would launch, and the description of its agent there.

use protocol::capability::settings::ModelCatalog;
use protocol::client::{ApiError, ErrorCode, admin};
use protocol::model::{
    AgentDescription, AgentKind, ModelList, ModelListKey, SessionSettings, SessionSettingsChange, WorkspaceId,
};
use std::sync::Arc;

use super::super::Hub;
use super::model_lists::Lookup;
use crate::store::SessionLogin;

impl Hub {
    /// The list `asked` names, as `ModelsFor` says.
    pub(crate) async fn models_for(self: &Arc<Self>, asked: admin::ModelsFor) -> Result<ModelList, ApiError> {
        if !self.agent(asked.agent)?.discovers_models() {
            let message = "this agent's description says what it offers";
            return Err(ApiError::new(ErrorCode::InvalidRequest, message));
        }
        let lookup = self.model_lookup(asked.agent, asked.target).await?;
        self.ensure_model_list(lookup, asked.again).await
    }

    /// What a new chat in `workspace` runs with: its agent's defaults there, changed by `options`, once its agent is
    /// offered them.
    pub(crate) async fn initial_settings(
        self: &Arc<Self>,
        agent: AgentKind,
        workspace: WorkspaceId,
        options: SessionSettingsChange,
    ) -> Result<SessionSettings, ApiError> {
        let description = self.agent_for(agent, admin::ModelTarget::Workspace(workspace)).await?;
        let mut defaults = description.defaults.clone();
        if let Some(model) = &options.model {
            defaults.effort = description.effort_for_choice(model, None);
        }
        let settings = defaults.changed(options);
        description
            .takes(&settings.clone().into())
            .map_err(|e| ApiError::new(ErrorCode::InvalidRequest, e.to_string()))?;
        Ok(settings)
    }

    /// What `agent` is called and offers where `target` would launch: its kind's description, with the models the hub
    /// holds for there in place of its kind's, for an agent that discovers its own, once the agent has said them.
    pub(crate) async fn agent_for(
        self: &Arc<Self>,
        agent: AgentKind,
        target: admin::ModelTarget,
    ) -> Result<AgentDescription, ApiError> {
        let description = self.agent(agent)?.clone();
        if !description.discovers_models() {
            return Ok(description);
        }
        let catalog = self.model_catalog(self.model_lookup(agent, target).await?).await?;
        if catalog.offered.is_empty() {
            return Err(ApiError::new(ErrorCode::Unavailable, "the agent offered no models for this login"));
        }
        Ok(description.with_catalog(catalog))
    }

    /// Which list applies where `target` would launch: the project, the provider its workspace is on, or new ones go
    /// to, and the account whose login the launch would be handed.
    async fn model_lookup(self: &Arc<Self>, agent: AgentKind, target: admin::ModelTarget) -> Result<Lookup, ApiError> {
        let (repo, provider, on) = match target {
            admin::ModelTarget::Project(repo) => {
                let (provider, _) = self.providers.for_new_workspace()?;
                (repo, provider.clone(), SessionLogin::NoneYet)
            }
            admin::ModelTarget::Workspace(id) => {
                let workspace = self.workspace(&id).await?;
                (workspace.repo_id, workspace.provider, SessionLogin::NoneYet)
            }
            admin::ModelTarget::Session(id) => {
                let session = self.session_get(id).await?;
                if session.agent != agent {
                    return Err(ApiError::new(ErrorCode::InvalidRequest, "the chat runs a different agent"));
                }
                let workspace = self.workspace(&session.workspace_id).await?;
                (workspace.repo_id, workspace.provider, SessionLogin::of_record(&session))
            }
        };
        let origin = self.blocking({
            let repo = repo.clone();
            move |store| store.get_repo(&repo)
        });
        let origin = origin.await?.origin;
        let account = self.login_for(agent, on).await?.map(|handed| handed.account);
        let key = ModelListKey { agent, repo, provider, account, cli_version: None };
        Ok(Lookup { key, origin })
    }

    /// Asks the agent which models it offers, in the project's checkout on the list's provider, under its login.
    pub(super) async fn look_up_models(&self, lookup: &Lookup) -> Result<ModelCatalog, ApiError> {
        let Lookup { key, origin } = lookup;
        let unavailable = || ApiError::new(ErrorCode::Unavailable, format!("provider {} isn't set up", key.provider));
        let provider = self.providers.get(&key.provider).ok_or_else(unavailable)?;
        let on = key.account.clone().map_or(SessionLogin::Ambient, SessionLogin::Hub);
        let login = self.login_for(key.agent, on).await?.map(|handed| handed.login);
        self.preflight_models(&provider, origin.clone(), key.agent, login).await
    }
}
