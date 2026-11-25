//! Settling the records of a workspace's other worktrees when the hub has a worker for the workspace again, as it
//! settles a workspace an earlier hub left being archived: from what's there.

use std::sync::Arc;

use crate::spans;
use protocol::client::ErrorCode;
use protocol::model::{ChildStatus, ChildWorktree, WorkspaceId};
use protocol::worker::{WorktreeThere, WorktreeThereReply};
use tracing::{Instrument as _, debug, info};

use super::{Making, add, watch_child};
use crate::hub::Hub;
use crate::worker_link::WorkerLink;

impl Hub {
    /// Settles the workspace's other worktrees with `link`, on a task of its own, since settling waits for the
    /// workspace's gate, which archiving holds while it starts a worker.
    pub(in crate::hub) fn settle_children_later(self: &Arc<Self>, id: &WorkspaceId, link: &Arc<WorkerLink>) {
        let (hub, id, link) = (self.clone(), id.clone(), link.clone());
        let span = spans::workspace(&id);
        tokio::spawn(async move { hub.settle_children(&id, &link).await }.instrument(span));
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
            Err(e) => {
                return debug!(
                    name: "child.worktrees_unfound",
                    workspace_id = %id,
                    error = e.message,
                    "Couldn't find the worktrees to settle"
                );
            }
        };
        let source = match self.workspace_placement(ready.id()).await {
            Ok(placement) => placement.source,
            Err(e) => {
                return debug!(
                    name: "child.placement_unread",
                    workspace_id = %id,
                    error = e.message,
                    "Couldn't read the placement of the worktrees to settle"
                );
            }
        };
        // As an existing branch, so the worker answers with what's there, and makes nothing new.
        let making = Making { link, source, own: ready.path(), base: None };
        for child in children {
            let there = match link.call(WorktreeThere { path: child.path.clone() }).await {
                Ok(WorktreeThereReply { there }) => there,
                // The worker can't say, such as one that's gone. Left for the next.
                Err(e) => {
                    return debug!(
                        name: "child.unasked",
                        workspace_id = %id,
                        error = e.message,
                        "Couldn't ask whether a worktree is there"
                    );
                }
            };
            self.settle_child(&making, &child, there).await;
        }
    }

    /// Settles `child`'s record, which its worker says is `there` or isn't.
    async fn settle_child(&self, making: &Making<'_>, child: &ChildWorktree, there: bool) {
        let settled = match (&child.status, there) {
            (ChildStatus::Ready, true) => {
                watch_child(making.link, child).await;
                Ok(())
            }
            (ChildStatus::Ready, false) => {
                info!(
                    name: "child.gone",
                    path = child.path,
                    "A worktree recorded as there has gone, so it's recorded as removed"
                );
                let id = child.id.clone();
                self.blocking(move |store| store.child_removed(&id)).await.map(drop)
            }
            // The worker makes it again if git was stopped part way through it. Only its saying that something
            // else is there forgets the record: after any other error the worktree may be this one, and the next
            // worker to connect is asked again.
            (ChildStatus::Creating, true) => match add(making, child).await {
                Ok(added) => self.made(making.link, child, added).await.map(drop),
                Err(e) if e.code == ErrorCode::NameTaken => self.discard(child).await,
                Err(e) => Err(e),
            },
            (ChildStatus::Creating, false) => self.discard(child).await,
            (ChildStatus::Removed { .. } | ChildStatus::Unknown, _) => Ok(()),
        };
        if let Err(e) = settled {
            debug!(
                name: "child.unsettled",
                path = child.path,
                error = e.message,
                "Couldn't settle a worktree's record"
            );
        }
    }
}
