//! Following what the hub says of GitHub, of what every view shows: a workspace at its own level while a view shows
//! its pull request in full, which the hub then reads more often, and each issue or pull request a chip shows, all
//! through one subscription, which changes what it follows in one request as what's shown changes. What the hub says of
//! every workspace's own pull request comes from following the hub, as [`crate::RecordsFeed`] does. What a workspace's
//! level sends, its own observations, such as its changed files, goes into the copies of the hub's records.

use parking_lot::Mutex;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::Follow;
use protocol::model::{Level, WorkspaceId};

use super::held::GithubHeld;
use crate::client::feed::Reaches;
use crate::client::follow::Source;
use crate::client::subscribing::Changing;
use crate::client::{Client, Result, Subscription, SubscriptionItem};

/// What changes what the subscription on this connection follows. What a workspace's level sends goes into the copies
/// of the hub's records the connection's replies go into, as they are when it comes, since a copy can start following
/// after this does, and what's held through it is let go of once it isn't followed.
#[derive(Default)]
pub(super) struct GithubSource {
    changing: Option<Changing>,
    /// The workspaces followed at their own level.
    workspaces: Vec<WorkspaceId>,
}

impl GithubSource {
    /// Takes `wanted` to be what's followed now, letting go of what was held only through a workspace no longer in it.
    /// A start the hub refused, such as of a workspace it no longer has, is left out until what's shown next changes,
    /// or a new connection, and the views show it as not known.
    fn followed_now(&mut self, client: &Client, wanted: &[Follow]) {
        let now = (wanted.iter())
            .filter_map(|follow| match follow.level() {
                Level::Workspace { id } => Some(id),
                Level::Hub
                | Level::Session { .. }
                | Level::Terminal { .. }
                | Level::Reference { .. }
                | Level::Unknown => None,
            })
            .collect::<Vec<_>>();
        let gone = std::mem::replace(&mut self.workspaces, now).into_iter().filter(|id| !self.workspaces.contains(id));
        let records = records(client);
        for id in gone {
            records.change(|records| records.let_go_of(&Level::Workspace { id: id.clone() }));
        }
    }
}

impl Source for GithubSource {
    type Held = GithubHeld;

    /// Each workspace's list comes before the reply to the request that follows it, so once it's answered, a
    /// workspace followed with none is one this hub sends no level of, as a hub from before workspaces had one, and
    /// what was held of it, from the connection before, is let go of.
    fn resume(&mut self, client: &Client, held: &Mutex<GithubHeld>) -> Result<Subscription> {
        self.changing = None;
        let wanted = followed(&held.lock());
        let (subscription, _refused) = client.follow_all(wanted.clone())?;
        self.followed_now(client, &wanted);
        let mut listed = Vec::new();
        for item in subscription.items.try_iter() {
            if let SubscriptionItem::Listed(of) = &item
                && let Listed::Workspace(observations) = &**of
            {
                listed.push(observations.workspace_id.clone());
            }
            self.apply(client, held, item)?;
        }
        let records = records(client);
        for id in self.workspaces.iter().filter(|id| !listed.contains(id)) {
            records.change(|records| records.let_go_of(&Level::Workspace { id: id.clone() }));
        }
        self.changing = Some(subscription.changing());
        Ok(subscription)
    }

    fn apply(&mut self, client: &Client, held: &Mutex<GithubHeld>, item: SubscriptionItem) -> Result<bool> {
        match item {
            SubscriptionItem::Github(update) => Ok(held.lock().apply(*update)),
            SubscriptionItem::Listed(listed) => {
                if let Listed::Workspace(observations) = *listed {
                    records(client).change(|records| records.apply_workspace_list(observations.clone()));
                }
                Ok(false)
            }
            SubscriptionItem::Notice(notice) => {
                records(client).change(|records| records.apply_notice((*notice).clone()));
                Ok(false)
            }
            SubscriptionItem::Event(_)
            | SubscriptionItem::Tail(_)
            | SubscriptionItem::Delta(_)
            | SubscriptionItem::Terminal(_)
            | SubscriptionItem::TerminalPicture(_) => Ok(false),
        }
    }

    /// Follows what's shown now, if that has changed, and lets go of what no view shows.
    fn asked(&mut self, connection: Option<&Client>, held: &Mutex<GithubHeld>, _: &dyn Fn()) -> Result<bool> {
        let wanted = {
            let mut held = held.lock();
            held.forget_unshown();
            followed(&held)
        };
        let (Some(client), Some(changing)) = (connection, &self.changing) else { return Ok(false) };
        changing.follow_only(client, wanted.clone())?;
        self.followed_now(client, &wanted);
        Ok(false)
    }

    fn reconnected(&mut self) {
        self.changing = None;
    }
}

/// What to follow for all that the views show.
fn followed(held: &GithubHeld) -> Vec<Follow> {
    let shown = held.shown();
    let workspaces = shown.closely.into_iter().map(|id| Follow::Workspace { id });
    workspaces.chain(shown.references.into_iter().map(|reference| Follow::Reference { reference })).collect()
}

/// The copies of the hub's records that `client`'s replies go into now.
fn records(client: &Client) -> Reaches {
    client.inner.folding.reaches()
}
