//! What the fake hub's agents that discover their own models offer, as its own hub would list them.

use protocol::capability::settings::ModelCatalog;
use protocol::client::{ApiError, ErrorCode, admin};
use protocol::model::{AgentKind, ModelList, ModelListKey, ModelLookup, Rev};

use super::World;

impl World {
    /// Has `agent` offer `catalog` in every repository there is now, on the local provider, under the machine's own
    /// login, as a hub that had asked it would list them.
    pub fn offer_models(&mut self, agent: AgentKind, catalog: &ModelCatalog) {
        for repo in &self.repos {
            let key = ModelListKey {
                agent,
                repo: repo.id.clone(),
                provider: "local".into(),
                account: None,
                cli_version: None,
            };
            let list =
                ModelList { key, catalog: Some(catalog.clone()), lookup: ModelLookup::Answered, rev: Rev::default() };
            self.model_lists.push(list);
        }
    }

    /// The list `asked` names, as `ModelsFor` says, of those the world has.
    pub(super) fn model_list_for(&self, asked: &admin::ModelsFor) -> Result<ModelList, ApiError> {
        let repo = match &asked.target {
            admin::ModelTarget::Project(repo) => repo.clone(),
            admin::ModelTarget::Workspace(id) => self.workspace_of(id)?,
            admin::ModelTarget::Session(id) => self.workspace_of(&self.session(id)?.workspace_id)?,
        };
        let listed = self.model_lists.iter().find(|list| list.key.agent == asked.agent && list.key.repo == repo);
        listed.cloned().ok_or_else(|| ApiError::new(ErrorCode::InvalidRequest, "the fake hub offers no models there"))
    }

    fn workspace_of(&self, id: &protocol::model::WorkspaceId) -> Result<protocol::model::RepoId, ApiError> {
        let workspace = self.workspaces.iter().find(|workspace| workspace.id == *id);
        workspace
            .map(|workspace| workspace.repo_id.clone())
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such workspace"))
    }
}
