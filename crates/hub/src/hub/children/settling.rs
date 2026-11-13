//! Settling the records of a workspace's other worktrees when the hub has a worker for the workspace again, as it
//! settles a workspace an earlier hub left being archived: from what's there.

use std::sync::Arc;

use protocol::client::ErrorCode;
use protocol::model::{ChildStatus, WorkspaceId};
use protocol::worker::{WorktreeThere, WorktreeThereReply};
use tracing::{debug, info};

use super::{Making, add, watch_child};
use crate::hub::Hub;
use crate::worker_link::WorkerLink;

impl Hub {
    /// Settles the workspace's other worktrees with `link`, on a task of its own, since settling waits for the
    /// workspace's gate, which archiving holds while it starts a worker.
    pub(in crate::hub) fn settle_children_later(self: &Arc<Self>, id: &WorkspaceId, link: &Arc<WorkerLink>) {
        let (hub, id, link) = (self.clone(), id.clone(), link.clone());
        tokio::spawn(async move { hub.settle_children(&id, &link).await });
    }

    /// Settles each of the workspace's other worktrees with `link`, the worker the workspace has now, and has it
    /// watch the branches of those that are there. One recorded as there that has gone, as when the last hub stopped
    /// before recording its removal, is recorded as removed. One recorded as being made, which the last hub or worker
    /// went before hearing of, is recorded as there if the worker has it, on its branch, and forgotten if nothing is
    /// there, or what's there isn't it, so its name is free.
    ///
    /// With the workspace's gate held to write, so none is being made meanwhile, and an ask heard again waits and
    /// finds its record settled.
    pub(in crate::hub) async fn settle_children(&self, id: &WorkspaceId, link: &WorkerLink) {
        let _settling = self.child_gate(id).write_owned().await;
        // One that isn't ready has no worktree to make another beside, and is settled once it has.
        let Ok(ready) = self.ready_workspace(id).await else { return };
        let owned = id.clone();
        let children = match self.blocking(move |store| store.children_of(&owned)).await {
            Ok(children) if children.is_empty() => return,
            Ok(children) => children,
            Err(e) => return debug!(%id, "finding the worktrees to settle: {}", e.message),
        };
        let repo = match self.repo_of(ready.workspace()).await {
            Ok(repo) => repo,
            Err(e) => return debug!(%id, "finding the repository of the worktrees to settle: {}", e.message),
        };
        // As an existing branch, so the worker answers with what's there, and makes nothing new.
        let making = Making { link, repo: &repo, own: ready.path(), base: None };
        for child in children {
            let there = match link.call(WorktreeThere { path: child.path.clone() }).await {
                Ok(WorktreeThereReply { there }) => there,
                // The worker can't say, as one from before the question, or one that's gone. Left for the next.
                Err(e) => return debug!(%id, "asking whether a worktree is there: {}", e.message),
            };
            let settled = match (&child.status, there) {
                (ChildStatus::Ready, true) => {
                    watch_child(link, &child).await;
                    Ok(())
                }
                (ChildStatus::Ready, false) => {
                    info!(path = child.path, "a worktree recorded as there has gone, so it's recorded as removed");
                    let id = child.id.clone();
                    self.blocking(move |store| store.child_removed(&id)).await.map(drop)
                }
                // The worker makes it again if git was stopped part way through it. Only its saying that something
                // else is there forgets the record: after any other error the worktree may be this one, and the next
                // worker to connect is asked again.
                (ChildStatus::Creating, true) => match add(&making, &child).await {
                    Ok(added) => self.made(link, &child, added).await.map(drop),
                    Err(e) if e.code == ErrorCode::NameTaken => self.discard(&child).await,
                    Err(e) => Err(e),
                },
                (ChildStatus::Creating, false) => self.discard(&child).await,
                (ChildStatus::Removed { .. } | ChildStatus::Unknown, _) => Ok(()),
            };
            if let Err(e) = settled {
                debug!(path = child.path, "settling a worktree's record: {}", e.message);
            }
        }
    }
}
