//! What GitHub says of each workspace's own pull request, worked out from the workspace's record, which says which pull
//! request is its own, from what the hub knows of its branches, and from what the cache keeps of that pull request: how it stands, at
//! the hub's level, its checks, history and stack, at the workspace's, and whether GitHub showed its repository at all.

use std::collections::{HashMap, HashSet};

use protocol::model::{
    OwnPullRequestDetail, OwnPullRequestSaid, OwnSummary, PullRequestDetail, PullRequestFound, Workspace, WorkspaceId,
};

use super::reads::{Heard, ReadEnded, ReadWent};
use crate::github_cache::{Item, Told};
use crate::hub::Hub;
use crate::hub::observations::{
    Keeper, Kind, WorkspacePullRequest, WorkspacePullRequestDetail, WorkspaceRepositoryUnseen,
};
use crate::hub::pull_requests::read_in_turn;
use crate::store::now_ms;

/// What the task knows of the workspaces it works these out for.
#[derive(Default)]
pub(super) struct Workspaces {
    /// Those clients followed, as the task last saw.
    followed: HashSet<WorkspaceId>,
    /// Those followed whose last read since they were followed was made and answered, so what's worked out of them is
    /// current, until a read fails or is held back, nothing follows them, or the hub stops reading them in turn, as
    /// when they're archived. Only what reads them says so, as its end, and nothing the task guesses.
    read: HashSet<WorkspaceId>,
    /// The pull request each one's record linked to as it was last worked out, as the cache keeps it, so a read the
    /// cache keeps of it works it out again.
    linked: HashMap<WorkspaceId, Item>,
}

impl Hub {
    /// Works out what GitHub says of `workspace`'s pull request from its record as it is now, and keeps each of its
    /// three kinds current from now if `current`, and otherwise as of the read it's worked out from. Returns the pull
    /// request its record links to, as the cache keeps it, if it links to one.
    async fn work_out_workspace(&self, workspace: &Workspace, current: bool) -> Option<Item> {
        let id = &workspace.id;
        self.keep_current_or_not::<WorkspacePullRequest>(id, current).await;
        self.keep_current_or_not::<WorkspacePullRequestDetail>(id, current).await;
        self.keep_current_or_not::<WorkspaceRepositoryUnseen>(id, current).await;
        if let Some(found) = self.known_of(workspace) {
            let ((said, said_at), (detail, detail_at)) = split(found);
            self.observation_worked_out::<WorkspacePullRequest>(id.clone(), said, said_at).await;
            self.observation_worked_out::<WorkspacePullRequestDetail>(id.clone(), detail, detail_at).await;
        } else {
            // Not looked for yet, or of a pull request the cache has forgotten, or that the record no longer links to.
            self.observation_gone::<WorkspacePullRequest>(id.clone()).await;
            self.observation_gone::<WorkspacePullRequestDetail>(id.clone()).await;
        }
        linked(workspace)
    }

    /// Keeps what a read of workspace `id`'s pull request that ended at `read_at_ms` heard of whether GitHub showed its
    /// repository, current if it's kept current, as [`Hub::work_out_workspace`] says. One it always showed, as most
    /// are, is kept of no workspace, which a client reads as showing it. A read that heard nothing of it, as one that
    /// failed otherwise does, leaves what's kept, so a hub that starts again, which has it as of the read that heard it,
    /// keeps it through such a read.
    async fn keep_unseen(&self, id: &WorkspaceId, heard: Heard, read_at_ms: i64) {
        let unseen = match heard {
            Heard::Unseen(unseen) => Some(unseen),
            Heard::Seen if self.observations.keeps::<WorkspaceRepositoryUnseen>(id) => None,
            Heard::Seen | Heard::Nothing => return,
        };
        self.observation_read::<WorkspaceRepositoryUnseen>(id.clone(), unseen, read_at_ms).await;
    }

    /// Keeps the observation `K` of `id` current from now, or as of its last read.
    async fn keep_current_or_not<K: Kind<Of = WorkspaceId>>(&self, id: &WorkspaceId, current: bool) {
        if current {
            self.observation_kept_by::<K>(id.clone(), Keeper::Hub).await;
        } else {
            self.observation_no_longer_kept_by::<K>(id.clone(), Keeper::Hub, now_ms()).await;
        }
    }
}

impl Workspaces {
    /// Whether what's worked out of `id` is current, as [`Workspaces::read`] says.
    fn current(&self, id: &WorkspaceId) -> bool {
        self.read.contains(id)
    }

    /// Works out `id` again, current as [`Workspaces::current`] says, but for one the hub doesn't read in turn, which
    /// nothing keeps up to date, so it's current again only from a read after it's read in turn again. Works out
    /// nothing of a workspace that's gone.
    async fn again(&mut self, hub: &Hub, id: &WorkspaceId) {
        let Ok(workspace) = hub.workspace(id).await else {
            self.linked.remove(id);
            return;
        };
        if !read_in_turn(&workspace) {
            self.read.remove(id);
        }
        match hub.work_out_workspace(&workspace, self.current(id)).await {
            Some(item) => self.linked.insert(id.clone(), item),
            None => self.linked.remove(id),
        };
    }

    /// Has what's worked out of `id` as of its last read, if it was current, as once nothing has read it since.
    async fn as_of_last_read(&mut self, hub: &Hub, id: &WorkspaceId) {
        if self.read.remove(id) {
            hub.keep_current_or_not::<WorkspacePullRequest>(id, false).await;
            hub.keep_current_or_not::<WorkspacePullRequestDetail>(id, false).await;
            hub.keep_current_or_not::<WorkspaceRepositoryUnseen>(id, false).await;
        }
    }

    /// Takes in what clients follow now. A workspace newly followed is worked out at once, as of the reads it's
    /// worked out from, and is current from the first read after that's made and answered. One no longer followed is as
    /// of its last read.
    pub(super) async fn follow_now(&mut self, hub: &Hub) {
        let now = hub.followers.workspaces();
        let newly: Vec<_> = now.difference(&self.followed).cloned().collect();
        let no_longer: Vec<_> = self.followed.difference(&now).cloned().collect();
        self.followed = now;
        for id in newly {
            self.again(hub, &id).await;
        }
        for id in no_longer {
            self.as_of_last_read(hub, &id).await;
        }
    }

    /// Takes in what the cache kept and forgot: each workspace whose pull request it was is worked out again.
    pub(super) async fn told(&mut self, hub: &Hub, told: &Told) {
        let of = |item: &Item| told.kept.contains(item) || told.forgotten.contains(item);
        let told: Vec<_> = self.linked.iter().filter(|(_, item)| of(item)).map(|(id, _)| id.clone()).collect();
        for id in told {
            self.again(hub, &id).await;
        }
    }

    /// Takes in that `workspace`'s record changed, as its link does, or that its branches did, which the hub keeps as
    /// an observation.
    pub(super) async fn recorded(&mut self, hub: &Hub, id: &WorkspaceId) {
        self.again(hub, id).await;
    }

    /// Takes in that the workspace `id` is gone.
    pub(super) async fn removed(&mut self, hub: &Hub, id: &WorkspaceId) {
        self.linked.remove(id);
        self.read.remove(id);
        // Again, since a read kept as it went may have worked it out after it was forgotten.
        hub.workspace_observations_gone(id).await;
    }

    /// Takes in how a read of a workspace's pull request went, and what it heard of the repository: one made and
    /// answered makes what's worked out of a workspace that's followed current, and any other has it as of its last
    /// read. One held back read nothing, so nothing is worked out again.
    pub(super) async fn read_ended(&mut self, hub: &Hub, ReadEnded { workspace: id, went, heard, at_ms }: ReadEnded) {
        match went {
            ReadWent::Read if self.followed.contains(&id) => {
                self.read.insert(id.clone());
            }
            ReadWent::Read | ReadWent::Failed => {
                self.read.remove(&id);
            }
            ReadWent::HeldBack => return self.as_of_last_read(hub, &id).await,
            ReadWent::Recounted => {}
        }
        self.again(hub, &id).await;
        hub.keep_unseen(&id, heard, at_ms).await;
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
