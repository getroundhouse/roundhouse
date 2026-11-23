//! The handles on a workspace followed at its own level, and what follows every one a handle is kept on.

use std::collections::BTreeMap;

use protocol::client::subscriptions::Follow;
use protocol::model::WorkspaceId;

use super::source::WorkspacesSource;
use crate::client::follow::{Following, Reach};
use crate::client::{Client, GithubFeed};
use crate::records::HubRecords;

/// Every workspace a handle is kept on, each with how many, and so what's followed.
#[derive(Default)]
pub(in crate::client) struct Wanted(BTreeMap<WorkspaceId, usize>);

impl Wanted {
    /// What to follow, which is each workspace once, however many handles are kept on it.
    pub(super) fn follows(&self) -> Vec<Follow> {
        self.0.keys().map(|id| Follow::Workspace { id: id.clone() }).collect()
    }

    fn want(&mut self, id: WorkspaceId) {
        *self.0.entry(id).or_default() += 1;
    }

    fn unwant(&mut self, id: &WorkspaceId) {
        if let Some(wanted) = self.0.get_mut(id) {
            *wanted -= 1;
            if *wanted == 0 {
                self.0.remove(id);
            }
        }
    }
}

/// What follows each workspace a [`FollowedWorkspace`] is kept on, for one copy of a hub's records, through one
/// subscription, which changes what it follows in one request as handles come and go.
#[derive(Clone)]
pub(in crate::client) struct Workspaces {
    follow: Following<Wanted>,
}

impl Workspaces {
    /// Starts following, on a thread of its own, with nothing followed until a handle is asked for. What's sent goes
    /// into `records`, and what GitHub says there into `github`, the GitHub feed of the same hub.
    pub(in crate::client) fn start(client: &Client, records: Reach<HubRecords>, github: GithubFeed) -> Self {
        let follow = Following::start(client, Wanted::default(), move || WorkspacesSource::new(records, github));
        Workspaces { follow }
    }

    /// Follows workspace `id` at its own level until the handle this returns is dropped, and every other kept on it.
    pub(in crate::client) fn follow(&self, id: &WorkspaceId) -> FollowedWorkspace {
        self.follow.quietly(|wanted| wanted.want(id.clone()));
        self.follow.wake();
        FollowedWorkspace { follow: self.follow.clone(), id: id.clone() }
    }

    /// Carries on on a new connection, following again every workspace a handle is kept on.
    pub(in crate::client) fn reconnect(&self, client: &Client) {
        self.follow.reconnect(client);
    }
}

/// A workspace followed at its own level, from [`crate::RecordsFeed::follow_workspace`], until this is dropped, and
/// every other handle on the same workspace. While it's followed, the hub sends what only its own pane draws, which
/// the copy of the hub's records holds, such as its changed files, as [`crate::Snapshot::workspace_changes`] reads
/// them, and its pull request's checks, as [`crate::GithubFeed::pull_request`] reads them. The hub's worker watches
/// its files, and the hub reads GitHub of it more often. Once nothing follows it, the copy holds none of that.
pub struct FollowedWorkspace {
    follow: Following<Wanted>,
    id: WorkspaceId,
}

impl FollowedWorkspace {
    /// The workspace followed.
    pub fn workspace(&self) -> &WorkspaceId {
        &self.id
    }
}

impl Drop for FollowedWorkspace {
    fn drop(&mut self) {
        self.follow.quietly(|wanted| wanted.unwant(&self.id));
        self.follow.wake();
    }
}
