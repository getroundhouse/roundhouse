//! Following what the hub says of GitHub, of what every view shows: a workspace at its own level while a view shows
//! its pull request in full, which the hub then reads more often, and each issue or pull request a chip shows, all
//! through one subscription, which changes what it follows in one request as what's shown changes. What the hub says of
//! every workspace's own pull request comes from following the hub, as [`crate::RecordsFeed`] does.

use parking_lot::Mutex;
use protocol::client::subscriptions::Follow;
use protocol::model::{Level, WorkspaceId};

use super::held::GithubHeld;
use crate::client::feed::Reaches;
use crate::client::follow::Source;
use crate::client::subscribing::Changing;
use crate::client::{Client, Result, Subscription, SubscriptionItem};

/// What changes what the subscription on this connection follows, and where what's held through a workspace's level
/// is let go of once it isn't followed.
#[derive(Default)]
pub(super) struct GithubSource {
    changing: Option<Changing>,
    records: Option<Reaches>,
    /// The workspaces followed at their own level.
    workspaces: Vec<WorkspaceId>,
}

impl GithubSource {
    /// Takes `wanted` to be what's followed now, letting go of what was held only through a workspace no longer in it.
    /// A start the hub refused, such as of a workspace it no longer has, is left out until what's shown next changes,
    /// or a new connection, and the views show it as not known.
    fn followed_now(&mut self, wanted: &[Follow]) {
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
        if let Some(records) = &self.records {
            for id in gone {
                records.change(|records| records.let_go_of(&Level::Workspace { id: id.clone() }));
            }
        }
    }
}

impl Source for GithubSource {
    type Held = GithubHeld;

    fn resume(&mut self, client: &Client, held: &Mutex<GithubHeld>) -> Result<Subscription> {
        self.changing = None;
        self.records = Some(client.inner.folding.reaches());
        let wanted = followed(&held.lock());
        let (subscription, _refused) = client.follow_all(wanted.clone())?;
        self.followed_now(&wanted);
        self.changing = Some(subscription.changing());
        Ok(subscription)
    }

    fn apply(&mut self, _: &Client, held: &Mutex<GithubHeld>, item: SubscriptionItem) -> Result<bool> {
        let SubscriptionItem::Github(update) = item else { return Ok(false) };
        Ok(held.lock().apply(*update))
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
        self.followed_now(&wanted);
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
