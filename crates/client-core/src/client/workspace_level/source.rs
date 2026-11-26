//! Following every workspace a handle is kept on, through one subscription, and taking what each sends into the copy of
//! the hub's records.

use parking_lot::Mutex;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::Follow;
use protocol::model::{Level, WorkspaceId};

use super::followed::Wanted;
use crate::client::follow::{Reach, Source};
use crate::client::subscribing::Changing;
use crate::client::{Client, Result, Subscription, SubscriptionItem};
use crate::records::HubRecords;

/// What changes what the subscription on this connection follows, and where what it sends goes. What's held through a
/// workspace is let go of once it isn't followed.
pub(super) struct WorkspacesSource {
    records: Reach<HubRecords>,
    changing: Option<Changing>,
    /// What was last asked for on this connection, which isn't asked for again until it changes, so a start the hub
    /// refused isn't sent again each time a handle comes or goes on a workspace already followed.
    asked_for: Vec<Follow>,
    /// The workspaces followed on this connection.
    followed: Vec<WorkspaceId>,
}

impl WorkspacesSource {
    pub(super) fn new(records: Reach<HubRecords>) -> Self {
        WorkspacesSource { records, changing: None, asked_for: Vec::new(), followed: Vec::new() }
    }

    /// Takes `wanted` to be what's followed now, letting go of what was held only through a workspace no longer in it.
    /// A start the hub refused, such as of a workspace it no longer has, is left out until the workspaces followed
    /// change, as a handle on another comes or the last on one goes, or a new connection, and what's read of it is as of
    /// a workspace nothing follows.
    fn followed_now(&mut self, wanted: &[Follow]) {
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
    /// workspace followed with none is one the hub refused, such as one it no longer has, and what was held of it, from
    /// the connection before, is let go of.
    fn resume(&mut self, client: &Client, held: &Mutex<Wanted>) -> Result<Subscription> {
        self.changing = None;
        let wanted = held.lock().follows();
        let (subscription, _refused) = client.follow_all(wanted.clone())?;
        self.followed_now(&wanted);
        self.asked_for.clone_from(&wanted);
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

    /// Takes what a workspace's level sends into the copy of the hub's records, which tells its own readers when that
    /// changes it. What's sent of a workspace no longer followed, which was already on its way as it was let go of, is
    /// dropped, so nothing brings back what letting go of it let go of.
    fn apply(&mut self, _: &Client, _: &Mutex<Wanted>, item: SubscriptionItem) -> Result<bool> {
        let Some(Level::Workspace { id }) = item.level() else { return Ok(false) };
        if !self.followed.contains(&id) {
            return Ok(false);
        }
        match item {
            SubscriptionItem::Listed(listed) => {
                let Listed::Workspace(observations) = *listed else { return Ok(false) };
                self.records.change(|records| records.apply_workspace_list(observations));
            }
            SubscriptionItem::Notice(notice) => {
                self.records.change(|records| records.apply_notice(*notice));
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
        if wanted == self.asked_for {
            return Ok(false);
        }
        changing.follow_only(client, wanted.clone())?;
        self.followed_now(&wanted);
        self.asked_for = wanted;
        Ok(false)
    }

    fn reconnected(&mut self) {
        self.changing = None;
    }
}
