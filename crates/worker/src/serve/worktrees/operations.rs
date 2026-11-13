use super::super::State;
use crate::branch_watch::Whose;
use crate::{branch_watch, git, setup};
use protocol::client::ApiError;
use protocol::worker::{
    AddChildWorktree, ChildWorktreeAdded, NamesGitHas, NamesGitHasReply, PrepareWorkspace, RemoveWorkspace,
    RestoreWorkspace, WorkerResponse, WorktreeThere, WorktreeThereReply,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::oneshot;
use tokio::task::AbortHandle;

/// What's watching the branches of each of the workspace's worktrees, so a later watch of one replaces it, and
/// removing one stops its watch.
#[derive(Default)]
pub(in super::super) struct Watches {
    pub(super) own: Option<AbortHandle>,
    pub(super) children: HashMap<PathBuf, AbortHandle>,
}

impl Watches {
    pub(super) fn start(&mut self, path: PathBuf, whose: Whose, watch: AbortHandle) {
        let earlier = match whose {
            Whose::Own => self.own.replace(watch),
            Whose::Child => self.children.insert(path, watch),
        };
        if let Some(earlier) = earlier {
            earlier.abort();
        }
    }

    pub(super) fn stop(&mut self, path: &Path) {
        if let Some(watching) = self.children.remove(path) {
            watching.abort();
        }
    }

    /// So they stop now, rather than once whatever they're reading is done.
    pub(in super::super) fn stop_all(&mut self) {
        self.own.take().into_iter().chain(self.children.drain().map(|(_, watching)| watching)).for_each(|w| w.abort());
    }
}

pub(in super::super) async fn names_git_has(state: &State, asked: NamesGitHas) -> Result<WorkerResponse, ApiError> {
    let names = git::names_git_has(&state.config.workspaces_root, &asked.repo_name, &asked.repo_path).await?;
    Ok(WorkerResponse::NamesGitHas(NamesGitHasReply { names }))
}

pub(in super::super) async fn prepare(state: &State, asked: PrepareWorkspace) -> Result<WorkerResponse, ApiError> {
    let PrepareWorkspace { repo_name, repo_path, workspace_name, branch, base_ref } = asked;
    let root = &state.config.workspaces_root;
    let ready = git::prepare_workspace(root, &repo_name, &repo_path, &workspace_name, &branch, base_ref.as_deref());
    Ok(WorkerResponse::WorkspaceReady(ready.await?))
}

pub(in super::super) async fn restore(state: &State, asked: RestoreWorkspace) -> Result<WorkerResponse, ApiError> {
    let _alone = state.moves.alone(Path::new(&asked.path)).await;
    git::restore_workspace(&asked.repo_path, &asked.path, &asked.branch).await?;
    Ok(WorkerResponse::Ok)
}

/// Removes the worktree at the path asked, the workspace's own or another made for it, and then stops watching
/// another's branches, so a removal that's refused leaves it watched.
pub(in super::super) async fn remove(state: &State, asked: RemoveWorkspace) -> Result<WorkerResponse, ApiError> {
    let path = Path::new(&asked.path);
    let _alone = state.moves.alone(path).await;
    git::remove_workspace(&asked, &state.config.deleter).await?;
    state.watches.lock().stop(path);
    Ok(WorkerResponse::Ok)
}

pub(in super::super) async fn there(state: &State, asked: WorktreeThere) -> Result<WorkerResponse, ApiError> {
    let _alone = state.moves.alone(Path::new(&asked.path)).await;
    let there = git::worktree_there(&asked.path).await?;
    Ok(WorkerResponse::WorktreeThere(WorktreeThereReply { there }))
}

/// Makes another worktree for the workspace's work where the hub asked, once nothing else is making or removing it,
/// and copies the ignored files setup copies into it. One made there before, as when a new hub asks again what the
/// last didn't hear back about, is answered with, and has copied what it's missing.
pub(in super::super) async fn add_child(state: &State, asked: AddChildWorktree) -> Result<WorkerResponse, ApiError> {
    let (repo, path) = (Path::new(&asked.repo_path), Path::new(&asked.path));
    let _alone = state.moves.alone(path).await;
    let mut warnings = Vec::new();
    let commit = git::add_child_worktree(Path::new(&asked.own), path, &asked.on, &mut warnings).await?;
    // Copying skips what's there, so one made before keeps what's changed in it since.
    let settings = setup::load_settings(repo, path, state.config.home.as_deref(), &mut warnings);
    let copied = setup::copy_local_files(repo, path, &settings, &mut warnings).await;
    let branches = git::branches::read_now(path).await.ok();
    Ok(WorkerResponse::ChildWorktreeAdded(ChildWorktreeAdded { commit, copied, warnings, branches }))
}

/// Watches the branches of the worktree at `path`, and answers once the first frame of them is queued, ahead of this
/// reply on the same stream, or the watch found it can't read them.
pub(in super::super) async fn watch_branches(
    state: &State,
    path: String,
    whose: Whose,
) -> Result<WorkerResponse, ApiError> {
    let (first, sent) = oneshot::channel();
    let path = PathBuf::from(path);
    let watching = branch_watch::watch(path.clone(), whose, state.outbox.clone(), state.stopping.subscribe(), first);
    state.watches.lock().start(path, whose, tokio::spawn(watching).abort_handle());
    let _ = sent.await;
    Ok(WorkerResponse::Ok)
}
