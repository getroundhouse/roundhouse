//! What GitHub says of each workspace's own pull request, worked out from the workspace's record, which says which pull
//! request is its own and what branches it has, and from what the cache keeps of that pull request: how it stands, at
//! the hub's level, its checks, history and stack, at the workspace's, and whether GitHub showed its repository at all.

use std::collections::{HashMap, HashSet};

use protocol::model::{
    OwnPullRequestDetail, OwnPullRequestSaid, OwnSummary, PullRequestDetail, PullRequestFound, Workspace, WorkspaceId,
};

use super::observing::logged;
use crate::github_cache::{Item, Told};
use crate::hub::Hub;
use crate::hub::observations::{Kind, WorkspacePullRequest, WorkspacePullRequestDetail, WorkspaceRepositoryUnseen};
use crate::store::now_ms;

/// What the task knows of the workspaces it works these out for.
#[derive(Default)]
pub(super) struct Workspaces {
    /// Those clients followed, as the task last saw.
    followed: HashSet<WorkspaceId>,
    /// Those whose last read failed, so what's kept of them is as of the read before, though they're followed.
    failed: HashSet<WorkspaceId>,
    /// The pull request each one's record linked to as it was last worked out, as the cache keeps it, so a read the
    /// cache keeps of it works it out again.
    linked: HashMap<WorkspaceId, Item>,
}

impl Hub {
    /// Works out what GitHub says of workspace `id`'s pull request from its record as it is now, and keeps it current
    /// from now if `current`, and otherwise as of the read it's worked out from. Returns the pull request its record
    /// links to, as the cache keeps it, if it links to one. Works out nothing of a workspace that's gone.
    async fn work_out_workspace(&self, id: &WorkspaceId, current: bool) -> Option<Item> {
        let workspace = self.workspace(id).await.ok()?;
        self.keep_current_or_not::<WorkspacePullRequest>(id, current).await;
        self.keep_current_or_not::<WorkspacePullRequestDetail>(id, current).await;
        if let Some(found) = self.known_of(&workspace) {
            let ((said, said_at), (detail, detail_at)) = split(found);
            logged(self.observation_worked_out::<WorkspacePullRequest>(id.clone(), said, said_at).await);
            logged(self.observation_worked_out::<WorkspacePullRequestDetail>(id.clone(), detail, detail_at).await);
        } else {
            // Not looked for yet, or of a pull request the cache has forgotten, or that the record no longer links to.
            logged(self.observation_gone::<WorkspacePullRequest>(id.clone()).await);
            logged(self.observation_gone::<WorkspacePullRequestDetail>(id.clone()).await);
        }
        linked(&workspace)
    }

    /// Works out whether GitHub showed workspace `id`'s repository, as the hub last heard. One it always showed, as
    /// most are, is kept of no workspace, which a client reads as showing it. Only as a read of it ends, or the hub hears
    /// of it, since the hub keeps what it heard in memory alone, so a hub that starts again has heard nothing, and what
    /// the store kept of it from before is as of then until the next read.
    async fn work_out_unseen(&self, id: &WorkspaceId, current: bool) {
        let unseen = self.unseen_of(id);
        if unseen.is_none() && !self.observations.keeps::<WorkspaceRepositoryUnseen>(id) {
            return;
        }
        self.keep_current_or_not::<WorkspaceRepositoryUnseen>(id, current).await;
        logged(self.observation_worked_out::<WorkspaceRepositoryUnseen>(id.clone(), unseen, now_ms()).await);
    }

    /// Keeps the observation `K` of `id` current from now, or as of its last read.
    async fn keep_current_or_not<K: Kind<Of = WorkspaceId>>(&self, id: &WorkspaceId, current: bool) {
        let kept = if current {
            self.observation_kept_current::<K>(id.clone()).await
        } else {
            self.observation_no_longer_read_again::<K>(id.clone()).await
        };
        logged(kept);
    }
}

impl Workspaces {
    /// Whether what's worked out of `id` is current: something follows it, and its last read didn't fail.
    fn current(&self, id: &WorkspaceId) -> bool {
        self.followed.contains(id) && !self.failed.contains(id)
    }

    /// Works out `id` again, current as [`Workspaces::current`] says.
    async fn again(&mut self, hub: &Hub, id: &WorkspaceId) {
        match hub.work_out_workspace(id, self.current(id)).await {
            Some(item) => self.linked.insert(id.clone(), item),
            None => self.linked.remove(id),
        };
    }

    /// Takes in what clients follow now. A workspace newly followed is worked out at once, as of the reads it's
    /// worked out from, and is current from the read following it starts. One no longer followed is as of its last
    /// read.
    pub(super) async fn follow_now(&mut self, hub: &Hub) {
        let now = hub.followers.workspaces();
        for id in now.difference(&self.followed) {
            self.failed.remove(id);
            if let Some(item) = hub.work_out_workspace(id, false).await {
                self.linked.insert(id.clone(), item);
            }
            hub.keep_current_or_not::<WorkspacePullRequest>(id, true).await;
            hub.keep_current_or_not::<WorkspacePullRequestDetail>(id, true).await;
            hub.keep_current_or_not::<WorkspaceRepositoryUnseen>(id, true).await;
        }
        for id in self.followed.difference(&now) {
            hub.keep_current_or_not::<WorkspacePullRequest>(id, false).await;
            hub.keep_current_or_not::<WorkspacePullRequestDetail>(id, false).await;
            hub.keep_current_or_not::<WorkspaceRepositoryUnseen>(id, false).await;
        }
        self.followed = now;
    }

    /// Takes in what the cache kept and forgot: each workspace whose pull request it was is worked out again.
    pub(super) async fn told(&mut self, hub: &Hub, told: &Told) {
        let of = |item: &Item| told.kept.contains(item) || told.forgotten.contains(item);
        let told: Vec<_> = self.linked.iter().filter(|(_, item)| of(item)).map(|(id, _)| id.clone()).collect();
        for id in told {
            self.again(hub, &id).await;
        }
    }

    /// Takes in that `workspace`'s record changed, as its link or its branches do.
    pub(super) async fn recorded(&mut self, hub: &Hub, workspace: &Workspace) {
        self.again(hub, &workspace.id).await;
    }

    /// Takes in that the workspace `id` is gone.
    pub(super) async fn removed(&mut self, hub: &Hub, id: &WorkspaceId) {
        self.linked.remove(id);
        self.failed.remove(id);
        // Again, since a read kept as it went may have worked it out after it was forgotten.
        hub.workspace_observations_gone(id).await;
    }

    /// Takes in that a read of `id`'s pull request ended, failed or not.
    pub(super) async fn read_ended(&mut self, hub: &Hub, id: &WorkspaceId, failed: bool) {
        if failed {
            self.failed.insert(id.clone());
        } else {
            self.failed.remove(id);
        }
        self.again(hub, id).await;
        hub.work_out_unseen(id, self.current(id)).await;
    }

    /// Takes in that what the hub heard of whether GitHub showed `id`'s repository changed.
    pub(super) async fn unseen(&self, hub: &Hub, id: &WorkspaceId) {
        hub.work_out_unseen(id, self.current(id)).await;
    }

    /// Works out every workspace again, as as the hub starts, or after missing some of what it's told, once every
    /// observation of a workspace that's gone is forgotten, as one worked out after a removal the task missed is.
    pub(super) async fn all_again(&mut self, hub: &Hub) {
        hub.observations_of_no_workspace_gone().await;
        let Ok(workspaces) = hub.workspace_list(true).await else { return };
        self.linked.retain(|id, _| workspaces.iter().any(|workspace| workspace.id == *id));
        for workspace in workspaces {
            self.again(hub, &workspace.id).await;
        }
    }
}

/// The pull request `workspace`'s record links to, as the cache keeps it.
fn linked(workspace: &Workspace) -> Option<Item> {
    workspace.pull_request_linked().map(|link| Item::new(&link.repository, link.number))
}

/// What the hub knows of a workspace's pull request, as each level is sent it, each with when what it's worked out
/// from was read. For a branch with none, that's now, since what's kept of it says none, and not when.
fn split(found: PullRequestFound) -> ((OwnPullRequestSaid, i64), (OwnPullRequestDetail, i64)) {
    let Some(own) = found.pull_request else {
        let now = now_ms();
        return ((OwnPullRequestSaid { pull_request: None }, now), (OwnPullRequestDetail { pull_request: None }, now));
    };
    let pull_request = &own.pull_request;
    let summary_read_at = pull_request.summary.read_at_ms;
    let facets_read_at = [
        pull_request.history.as_ref().map(|read| read.read_at_ms),
        pull_request.stack.as_ref().map(|read| read.read_at_ms),
        pull_request.checks.as_ref().map(|read| read.read_at_ms),
    ];
    let detail_read_at = facets_read_at.into_iter().flatten().min().unwrap_or(summary_read_at);
    (
        (OwnPullRequestSaid { pull_request: Some(OwnSummary::of(&own)) }, summary_read_at),
        (OwnPullRequestDetail { pull_request: Some(PullRequestDetail::of(pull_request)) }, detail_read_at),
    )
}
