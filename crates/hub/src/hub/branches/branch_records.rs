use super::super::Hub;
use super::super::pull_requests::When;
use super::super::ready::ReadyWorkspace;
use crate::hub::observations::{ChildWorktreeBranches, ChildWorktreeKey, WorkspaceBranches};
use crate::store::now_ms;
use crate::worker_link::WorkerLink;
use protocol::client::ErrorCode;
use protocol::model::{Branches, ChildWorktreeId, Head, Upstream, WorkspaceId, WorkspaceStatus};
use protocol::worker::{WatchBranches, WorkerFrame};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

/// A worker reads the branches in milliseconds. Hitting this means something is wrong with it.
const FIRST_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// What a change to the branches means for the pull request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Moved {
    /// Another branch is checked out, so the pull request known may be another branch's.
    Switched,
    /// The branch checked out was pushed, so GitHub has something new of it.
    Pushed,
}

impl Hub {
    /// Has a ready workspace's worker watch its worktree's branches, which are current from now. Not waited on: the
    /// worker answers only that it's watching, and what it sees comes in frames of its own.
    ///
    /// Its other worktrees' records are settled, and those there watched, on a task of their own, and its worktree's
    /// files are watched too while a client follows the workspace, as each worker the hub takes on is told.
    pub(in super::super) async fn watch_branches(self: &Arc<Self>, ready: &ReadyWorkspace, link: &Arc<WorkerLink>) {
        self.branches_kept_current(ready.id()).await;
        let path = ready.path().to_owned();
        if let Err(e) = link.send(WatchBranches { path }).await {
            debug!(id = %ready.id(), "asking the worker to watch the branches: {}", e.message);
        }
        self.settle_children_later(ready.id(), link);
        self.changes_watching_soon(ready.id());
    }

    /// The same, waiting until the worker has sent what the branches are now, and the hub has kept them, for a
    /// worktree that's just come back, so the hub knows which branch it's on before it's announced. Whether it
    /// watches the worktree's files is told as the workspace's move ends, as [`Hub::end_move`] says.
    pub(in super::super) async fn watch_branches_from_now(
        self: &Arc<Self>,
        ready: &ReadyWorkspace,
        link: &Arc<WorkerLink>,
    ) {
        self.settle_children_later(ready.id(), link);
        self.branches_kept_current(ready.id()).await;
        let path = ready.path().to_owned();
        let first = tokio::time::timeout(FIRST_READ_TIMEOUT, link.call(WatchBranches { path })).await;
        let id = ready.id();
        match first {
            Ok(Ok(())) => {}
            Ok(Err(e)) => debug!(%id, "asking the worker to watch the branches: {}", e.message),
            Err(_) => warn!(%id, "the worker didn't read the branches within {FIRST_READ_TIMEOUT:?}"),
        }
    }

    /// What the workspace's worker read of the branches of one of its worktrees: its own, or another made for it.
    /// Gives back a frame that isn't either. Only from the worker the workspace has now, since one that's been
    /// replaced may still have frames on the way, which say less than the new one's.
    pub(crate) async fn branches_frame(
        self: &Arc<Self>,
        id: &WorkspaceId,
        link: &Arc<WorkerLink>,
        frame: WorkerFrame,
    ) -> Option<WorkerFrame> {
        let current = || link.is_current(&self.workers.lock(), id);
        if let WorkerFrame::Branches { branches } = frame {
            if current() {
                self.branches_read(id, branches).await;
                // Its first word of them, as the hub takes it on, or a checkout, a rebase or a reset, which can change
                // what the worktree adds and removes against its base.
                self.read_lines(id, link);
            }
            return None;
        }
        if let WorkerFrame::ChildBranches { path, branches } = frame {
            if current() {
                self.child_branches_read(id, path, branches).await;
            }
            return None;
        }
        Some(frame)
    }

    /// What the workspace's worker read of the branches of another of its worktrees, at `path`, which the hub keeps of
    /// that worktree, and sends nowhere.
    pub(super) async fn child_branches_read(&self, id: &WorkspaceId, path: String, branches: Branches) {
        let owned = id.clone();
        let child = match self.blocking(move |store| store.child_at(&owned, &path)).await {
            Ok(Some(child)) => child,
            // Removed meanwhile, which leaves what was read of it before.
            Ok(None) => return,
            Err(e) => return debug!(%id, "finding the worktree whose branches were read: {}", e.message),
        };
        self.child_branches_known(id, child.id, branches).await;
    }

    /// Keeps `branches` as what the workspace's other worktree `child` has, as of now.
    pub(in super::super) async fn child_branches_known(
        &self,
        id: &WorkspaceId,
        child: ChildWorktreeId,
        branches: Branches,
    ) {
        let of = ChildWorktreeKey { workspace_id: id.clone(), id: child };
        if let Err(e) = self.observation_read::<ChildWorktreeBranches>(of, branches, now_ms()).await {
            debug!(%id, "keeping another worktree's branches: {}", e.message);
        }
    }

    /// What the workspace's worker read of its own worktree's branches, which the hub keeps as the observation
    /// [`WorkspaceBranches`]. Reads the pull request again where what moved says it may be another, or have something
    /// new.
    pub(super) async fn branches_read(self: &Arc<Self>, id: &WorkspaceId, branches: Branches) {
        let before = self.observations.value_of::<WorkspaceBranches>(id);
        self.branches_known(id, branches.clone()).await;
        // Removed or archived while they were read, after that forgot what was kept, so what this kept goes too.
        match self.workspace(id).await {
            Err(e) if e.code == ErrorCode::NotFound => return self.workspace_observations_gone(id).await,
            Ok(workspace) if workspace.status == WorkspaceStatus::Archived => return self.branches_forgotten(id).await,
            Ok(_) | Err(_) => {}
        }
        // With nothing known before, as after the hub's cache was emptied, which pull request is the workspace's
        // couldn't be told, so it's read now it can. Otherwise a pull request found on another branch is read in turn
        // even so, merged or not.
        let Some(before) = before else { return self.read_pull_request_after_move(id, When::Always) };
        match moved(&before, &branches) {
            Some(Moved::Switched) => self.read_pull_request_after_move(id, When::OnAnotherBranch),
            Some(Moved::Pushed) => self.read_pull_request_after_move(id, When::Always),
            None => {}
        }
    }

    /// Keeps `branches` as what the workspace's own worktree has, as of now, or current while its worker watches them.
    pub(crate) async fn branches_known(&self, id: &WorkspaceId, branches: Branches) {
        if let Err(e) = self.observation_read::<WorkspaceBranches>(id.clone(), branches, now_ms()).await {
            warn!(%id, "keeping the workspace's branches: {}", e.message);
        }
    }

    /// What the hub knows of the workspace's branches, however fresh, or `None` while they aren't known, as before its
    /// worker has said them since the hub's cache was emptied.
    pub(crate) fn branches_of(&self, id: &WorkspaceId) -> Option<Branches> {
        self.observations.value_of::<WorkspaceBranches>(id)
    }

    /// Keeps `branches` as what the workspace's worktree had at `read_at_ms`, under no revision, telling nobody, for a
    /// test of what reads them outside `crate::hub`.
    #[cfg(test)]
    pub(crate) fn keep_branches_as_read(&self, id: &WorkspaceId, branches: Branches, read_at_ms: i64) {
        self.observations.keep_as_read::<WorkspaceBranches>(id.clone(), branches, read_at_ms);
    }

    /// Keeps that the workspace's worker watches its branches from now on, so what it reads of them is current.
    async fn branches_kept_current(&self, id: &WorkspaceId) {
        if let Err(e) = self.observation_kept_current::<WorkspaceBranches>(id.clone()).await {
            warn!(%id, "keeping that the workspace's branches are watched: {}", e.message);
        }
    }

    /// Keeps that nothing watches the workspace's branches any more, as its worker has gone, so they're as of now, since
    /// the worker would have said of any change until then.
    pub(in super::super) async fn branches_let_go(&self, id: &WorkspaceId) {
        if let Err(e) = self.observation_let_go::<WorkspaceBranches>(id.clone(), now_ms()).await {
            warn!(%id, "keeping that the workspace's branches are no longer watched: {}", e.message);
        }
    }

    /// Forgets the workspace's branches, as once it's archived, whose record keeps the branch it had checked out, and
    /// whose worker says them again once it's brought back. Its other worktrees' are kept, since a branch one of them
    /// had is still the workspace's, as `crate::hub::children` reads it.
    pub(in super::super) async fn branches_forgotten(&self, id: &WorkspaceId) {
        if let Err(e) = self.observation_gone::<WorkspaceBranches>(id.clone()).await {
            warn!(%id, "forgetting the workspace's branches: {}", e.message);
        }
    }
}

/// What moved for the pull request between two reads of the branches. A push changes where the branch checked out
/// is pushed, or brings what the worktree knows of it there up to the branch's own newest commit, which hasn't moved.
/// A fetch moves only what's there, such as `origin/main` under a branch made from it, which tracks it, a commit moves
/// only the branch, and a pull moves both to the same commit, so none is a push, and GitHub has nothing new of the
/// branch until one. Nor is a commit pushed before the branches are next read, which a turn's end reads instead. A
/// detached HEAD has no pull request.
pub(super) fn moved(before: &Branches, after: &Branches) -> Option<Moved> {
    if !matches!(after.head, Head::Branch { .. }) {
        return None;
    }
    if before.head != after.head {
        return Some(Moved::Switched);
    }
    // The same branch on both sides, since HEAD is.
    let (was, is) = (before.checked_out()?, after.checked_out()?);
    let (was_pushed, is_pushed) = (was.upstream.as_ref(), is.upstream.as_ref());
    let to = |upstream: &Upstream| (upstream.remote.clone(), upstream.name.clone());
    let elsewhere = was_pushed.map(to) != is_pushed.map(to);
    let caught_up = was.tip == is.tip
        && is_pushed.is_some_and(|pushed| {
            pushed.tip.as_ref() == Some(&is.tip) && was_pushed.map(|was| &was.tip) != Some(&pushed.tip)
        });
    (elsewhere || caught_up).then_some(Moved::Pushed)
}
