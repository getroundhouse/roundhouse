//! Resolving a new workspace's [`WorkspacePlacement`] from its repository and provider's [`WorkerEnvironment`], and
//! building the `CONDUCTOR_*` environment variables scripts, agents and terminals receive from that recorded placement.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ProviderName, Repo, RepoUrl, Workspace, WorkspaceId};
use protocol::provider::{ClonePersistence, WorkerEnvironment, WorkerFilesystem};
use protocol::worker::RepoSource;
use providers::Provider;

use super::Hub;
use crate::error::provider_failed;
use crate::store::WorkspacePlacement;

/// Resolves the repository source and worker environment for a new workspace of `repo` on a provider named
/// `provider_name` declaring `environment`, where a machine with no checkout clones `clone`, or can't, as it says.
/// Refuses any unknown axis so the hub never guesses. On the hub's machine, a checkout is used when the repository has
/// one (so local ignored files and settings come along), and otherwise the address to clone. On a separate machine,
/// only an address is ever sent, never a path on the hub's disk.
pub(super) fn resolve_placement(
    repo: &Repo,
    clone: Result<RepoUrl, ApiError>,
    provider_name: &str,
    environment: WorkerEnvironment,
) -> Result<WorkspacePlacement, ApiError> {
    if environment.clones == ClonePersistence::Unknown {
        return Err(unknown_environment(provider_name));
    }
    let source = match (environment.filesystem, repo.origin.checkout()) {
        (WorkerFilesystem::Hub, Some(path)) => RepoSource::Checkout { path: path.to_owned() },
        (WorkerFilesystem::Hub | WorkerFilesystem::Separate, _) => {
            RepoSource::Clone { url: clone?.as_str().to_owned() }
        }
        (WorkerFilesystem::Unknown, _) => return Err(unknown_environment(provider_name)),
    };
    Ok(WorkspacePlacement { source, environment })
}

/// What scripts, agents and terminals see, under the names Conductor uses, so that scripts written for it work.
/// `CONDUCTOR_IS_LOCAL` is `1` on the hub's machine and `0` on a separate machine. `CONDUCTOR_ROOT_PATH` is set only
/// when the workspace was made from a checkout on the hub's machine, and omitted for cloned workspaces (whether on a
/// separate machine or a remote-only repository on the hub), since pointing it at the worktree breaks scripts that
/// copy or symlink from root into `.`.
pub(crate) fn workspace_env_for(workspace: &Workspace, placement: &WorkspacePlacement) -> Vec<(String, String)> {
    let base = workspace.base_ref.as_deref().unwrap_or("main");
    let is_local = match placement.environment.filesystem {
        WorkerFilesystem::Hub => "1",
        WorkerFilesystem::Separate | WorkerFilesystem::Unknown => "0",
    };
    let mut env = vec![
        ("CONDUCTOR_WORKSPACE_NAME".to_string(), workspace.name.clone()),
        ("CONDUCTOR_WORKSPACE_ID".to_string(), workspace.id.to_string()),
        ("CONDUCTOR_DEFAULT_BRANCH".to_string(), base.trim_prefix("origin/").to_string()),
        ("CONDUCTOR_PORT".to_string(), workspace.port_base.to_string()),
        ("CONDUCTOR_IS_LOCAL".to_string(), is_local.to_string()),
    ];
    if let Some(root) = placement.source.checkout() {
        env.push(("CONDUCTOR_ROOT_PATH".to_string(), root.to_owned()));
    }
    if let Some(path) = &workspace.path {
        env.push(("CONDUCTOR_WORKSPACE_PATH".to_string(), path.clone()));
    }
    env
}

fn unknown_environment(provider_name: &str) -> ApiError {
    ApiError::new(
        ErrorCode::Unavailable,
        format!("provider {provider_name} uses a worker environment this hub doesn't know"),
    )
}

impl Hub {
    /// Queries `provider`'s worker environment and settles the [`WorkspacePlacement`] for a new workspace of `repo`.
    pub(crate) async fn resolve_workspace_placement(
        &self,
        repo: &Repo,
        provider: &dyn Provider,
    ) -> Result<WorkspacePlacement, ApiError> {
        let environment = provider.environment().await.map_err(|e| provider_failed(&e))?;
        let clone = self.clone_url_of(repo, &ProviderName::from(provider.name()));
        resolve_placement(repo, clone, provider.name(), environment)
    }

    /// Reads the [`WorkspacePlacement`] settled when workspace `id` was created, without contacting its provider.
    pub(crate) async fn workspace_placement(&self, id: &WorkspaceId) -> Result<WorkspacePlacement, ApiError> {
        let owned = id.clone();
        self.blocking(move |store| store.workspace_placement(&owned)).await
    }

    /// Builds the `CONDUCTOR_*` environment variables for `workspace` from its recorded placement.
    pub(crate) async fn workspace_env(&self, workspace: &Workspace) -> Result<Vec<(String, String)>, ApiError> {
        let placement = self.workspace_placement(&workspace.id).await?;
        Ok(workspace_env_for(workspace, &placement))
    }
}
