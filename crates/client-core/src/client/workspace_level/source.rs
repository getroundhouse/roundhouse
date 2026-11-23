//! Following every workspace a handle is kept on, through one subscription, and taking what each sends into the copy of
//! the hub's records.

use std::cell::Cell;

use parking_lot::Mutex;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::Follow as Start;
use protocol::model::{Level, WorkspaceId};

use super::followed::Wanted;
use crate::client::follow::{Reach, Source};
use crate::client::github::{github_keys, notice_key};
use crate::client::subscribing::Changing;
use crate::client::{Client, GithubFeed, Result, Subscription, SubscriptionItem};
use crate::records::HubRecords;

/// What changes what the subscription on this connection follows, and where what it sends goes. What's held through a
/// workspace is let go of once it isn't followed.
pub(super) struct WorkspacesSource {
    records: Reach<HubRecords>,
    /// The same hub's GitHub feed, which reads what GitHub says of a workspace's pull request from the copy, and is told
    /// when that changes.
    github: GithubFeed,
    changing: Option<Changing>,
    /// The workspaces followed on this connection.
    followed: Vec<WorkspaceId>,
}

impl WorkspacesSource {
    pub(super) fn new(records: Reach<HubRecords>, github: GithubFeed) -> Self {
        WorkspacesSource { records, github, changing: None, followed: Vec::new() }
    }

    /// Takes `wanted` to be what's followed now, letting go of what was held only through a workspace no longer in it.
    /// A start the hub refused, such as of a workspace it no longer has, is left out until a handle comes or goes, or a
    /// new connection, and what's read of it is as of a workspace nothing follows.
    fn followed_now(&mut self, wanted: &[Start]) {
        let now = (wanted.iter())
            .filter_map(|start| match start.level() {
                Level::Workspace { id } => Some(id),
                Level::Hub
                | Level::Session { .. }
                | Level::Terminal { .. }
                | Level::Reference { .. }
                | Level::Unknown => None,
            })
            .collect::<Vec<_>>();
        let gone = std::mem::replace(&mut self.followed, now).into_iter().filter(|id| !self.followed.contains(id));
        for id in gone.collect::<Vec<_>>() {
            self.records.change(|records| records.let_go_of(&Level::Workspace { id }));
        }
    }
}

impl Source for WorkspacesSource {
    type Held = Wanted;

    /// Each workspace's list comes before the reply to the request that follows it, so once it's answered, a
    /// workspace followed with none is one this hub sends no level of, as a hub from before workspaces had one, and
    /// what was held of it, from the connection before, is let go of.
    fn resume(&mut self, client: &Client, held: &Mutex<Wanted>) -> Result<Subscription> {
        self.changing = None;
        let wanted = held.lock().follows();
        let (subscription, _refused) = client.follow_all(wanted.clone())?;
        self.followed_now(&wanted);
        let mut listed = Vec::new();
        for item in subscription.items.try_iter() {
            if let SubscriptionItem::Listed(of) = &item
                && let Listed::Workspace(observations) = &**of
            {
                listed.push(observations.workspace_id.clone());
            }
            self.apply(client, held, item)?;
        }
        for id in self.followed.iter().filter(|id| !listed.contains(id)) {
            self.records.change(|records| records.let_go_of(&Level::Workspace { id: id.clone() }));
        }
        self.changing = Some(subscription.changing());
        Ok(subscription)
    }

    /// Takes what a workspace's level sends into the copy of the hub's records, telling the GitHub feed what it took in
    /// of what GitHub says, and hands it what's sent in the form from before observations, `GithubUpdate`.
    fn apply(&mut self, _: &Client, _: &Mutex<Wanted>, item: SubscriptionItem) -> Result<bool> {
        // Whether the copy changed, which it tells its own readers of.
        let changed = Cell::new(false);
        let change = |change: &dyn Fn(&mut HubRecords) -> bool| {
            self.records.change(|records| {
                let changed_it = change(records);
                changed.set(changed.get() || changed_it);
                changed_it
            });
        };
        match item {
            SubscriptionItem::Github(update) => self.github.take(*update),
            SubscriptionItem::Listed(listed) => {
                let Listed::Workspace(observations) = *listed else { return Ok(false) };
                let keys = github_keys(&observations.observations);
                change(&|records| records.apply_workspace_list(observations.clone()));
                self.github.observed(&keys, changed.get());
            }
            SubscriptionItem::Notice(notice) => {
                let key = notice_key(&notice);
                change(&|records| records.apply_notice((*notice).clone()));
                if let Some(key) = key {
                    self.github.observed(&[key], changed.get());
                }
            }
            SubscriptionItem::Event(_)
            | SubscriptionItem::Tail(_)
            | SubscriptionItem::Delta(_)
            | SubscriptionItem::Terminal(_)
            | SubscriptionItem::TerminalPicture(_) => {}
        }
        // What's held here is what handles want, which nothing the hub sends changes.
        Ok(false)
    }

    /// Follows every workspace a handle is kept on, if that has changed, and lets go of the rest.
    fn asked(&mut self, connection: Option<&Client>, held: &Mutex<Wanted>, _: &dyn Fn()) -> Result<bool> {
        let wanted = held.lock().follows();
        let (Some(client), Some(changing)) = (connection, &self.changing) else { return Ok(false) };
        changing.follow_only(client, wanted.clone())?;
        self.followed_now(&wanted);
        Ok(false)
    }

    fn reconnected(&mut self) {
        self.changing = None;
    }
}
