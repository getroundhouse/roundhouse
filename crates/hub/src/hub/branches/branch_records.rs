use super::super::Hub;
use super::super::pull_requests::When;
use super::super::ready::ReadyWorkspace;
use crate::store::BranchesSet;
use crate::worker_link::WorkerLink;
use protocol::client::HubNotice;
use protocol::model::{Branches, Head, Upstream, WorkspaceId};
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
    /// Has a ready workspace's worker watch its worktree's branches. Not waited on: the worker
    /// answers only that it's watching, and what it sees comes in frames of its own.
    ///
    /// Its other worktrees' records are settled, and those there watched, on a task of their own.
    pub(in super::super) async fn watch_branches(self: &Arc<Self>, ready: &ReadyWorkspace, link: &Arc<WorkerLink>) {
        let path = ready.path().to_owned();
        if let Err(e) = link.send(WatchBranches { path }).await {
            debug!(id = %ready.id(), "asking the worker to watch the branches: {}", e.message);
        }
        self.settle_children_later(ready.id(), link);
    }

    /// The same, waiting until the worker has sent what the branches are now, and the hub has recorded it, for a
    /// worktree that's just come back, so its record says which branch it's on before it's announced.
    pub(in super::super) async fn watch_branches_from_now(
        self: &Arc<Self>,
        ready: &ReadyWorkspace,
        link: &Arc<WorkerLink>,
    ) {
        self.settle_children_later(ready.id(), link);
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
                self.branches_read(id, link, branches).await;
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

    /// What the workspace's worker read of the branches of another of its worktrees, which go on that worktree's
    /// record.
    pub(super) async fn child_branches_read(&self, id: &WorkspaceId, path: String, branches: Branches) {
        let owned = id.clone();
        if let Err(e) = self.blocking(move |store| store.set_child_branches(&owned, &path, &branches)).await {
            debug!(%id, "recording another worktree's branches: {}", e.message);
        }
    }

    /// What the workspace's worker read of its own worktree's branches.
    pub(super) async fn branches_read(self: &Arc<Self>, id: &WorkspaceId, link: &Arc<WorkerLink>, branches: Branches) {
        let (owned, after) = (id.clone(), branches.clone());
        let (before, workspace) = match self.blocking(move |store| store.set_workspace_branches(&owned, &after)).await {
            Ok(BranchesSet { before, workspace: Some(workspace) }) => (before, workspace),
            Ok(BranchesSet { workspace: None, .. }) => return,
            Err(e) => {
                warn!(%id, "recording the branches: {}", e.message);
                return;
            }
        };
        self.notify(HubNotice::Workspace(workspace));
        // With nothing recorded before, there's nothing to say what moved. A pull request found on another branch is
        // read in turn even so, merged or not.
        match before.and_then(|before| moved(&before, &branches)) {
            Some(Moved::Switched) => self.read_pull_request_after_move(id, link.clone(), When::OnAnotherBranch),
            Some(Moved::Pushed) => self.read_pull_request_after_move(id, link.clone(), When::Always),
            None => {}
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
