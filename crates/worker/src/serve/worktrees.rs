//! The workspace's worktree as a whole: which names git already has, before one is made, making it, putting it back,
//! removing it, and whether it's there.

use protocol::client::ApiError;
use protocol::worker::{
    NamesGitHas, NamesGitHasReply, PrepareWorkspace, RemoveWorkspace, RestoreWorkspace, WorkerResponse, WorktreeThere,
    WorktreeThereReply,
};

use super::State;
use crate::git;

pub(super) async fn names_git_has(state: &State, asked: NamesGitHas) -> Result<WorkerResponse, ApiError> {
    let names = git::names_git_has(&state.config.workspaces_root, &asked.repo_name, &asked.repo_path).await?;
    Ok(WorkerResponse::NamesGitHas(NamesGitHasReply { names }))
}

pub(super) async fn prepare(state: &State, asked: PrepareWorkspace) -> Result<WorkerResponse, ApiError> {
    let PrepareWorkspace { repo_name, repo_path, workspace_name, branch, base_ref } = asked;
    let root = &state.config.workspaces_root;
    let ready = git::prepare_workspace(root, &repo_name, &repo_path, &workspace_name, &branch, base_ref.as_deref());
    Ok(WorkerResponse::WorkspaceReady(ready.await?))
}

pub(super) async fn restore(state: &State, asked: RestoreWorkspace) -> Result<WorkerResponse, ApiError> {
    let _moving = state.moving.lock().await;
    git::restore_workspace(&asked.repo_path, &asked.path, &asked.branch).await?;
    Ok(WorkerResponse::Ok)
}

pub(super) async fn remove(state: &State, asked: RemoveWorkspace) -> Result<WorkerResponse, ApiError> {
    let _moving = state.moving.lock().await;
    git::remove_workspace(&asked.repo_path, &asked.path, asked.force, &state.config.deleter).await?;
    Ok(WorkerResponse::Ok)
}

pub(super) async fn there(state: &State, asked: WorktreeThere) -> Result<WorkerResponse, ApiError> {
    let _moving = state.moving.lock().await;
    let there = git::worktree_there(&asked.path).await;
    Ok(WorkerResponse::WorktreeThere(WorktreeThereReply { there }))
}
