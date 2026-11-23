//! Following what the hub says of GitHub, of what every view shows: a workspace at its own level while a view shows
//! its pull request in full, which the hub then reads more often, and each issue or pull request a chip shows, all
//! through one subscription, which changes what it follows in one request as what's shown changes. What the hub says of
//! every workspace's own pull request comes from following the hub, as [`crate::RecordsFeed`] does. What a workspace's
//! level sends, its own observations, such as its changed files and what GitHub says of its pull request, goes into the
//! copy of the hub's records, as what an issue's or a pull request's level sends as an observation does, and what's
//! sent in the form from before observations, `GithubUpdate`, is held here.

use parking_lot::Mutex;
use std::cell::Cell;

use protocol::client::github::GithubUpdate;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::Follow;
use protocol::model::{Level, ObservationKey, ReferenceAsk, WorkspaceId};

use super::held::{GithubHeld, github_keys, notice_key};
use crate::client::follow::{Reach, Source};
use crate::client::subscribing::Changing;
use crate::client::{Client, Result, Subscription, SubscriptionItem};
use crate::records::HubRecords;

/// What changes what the subscription on this connection follows. What a workspace's level sends goes into the copy of
/// the hub's records, and what's held through it is let go of once it isn't followed.
pub(super) struct GithubSource {
    /// The copy of the same hub's records, which the GitHub feed reads.
    records: Reach<HubRecords>,
    changing: Option<Changing>,
    /// The workspaces followed at their own level.
    workspaces: Vec<WorkspaceId>,
    /// The issues and pull requests followed at their own level.
    references: Vec<ReferenceAsk>,
}

impl GithubSource {
    pub(super) fn new(records: Reach<HubRecords>) -> Self {
        GithubSource { records, changing: None, workspaces: Vec::new(), references: Vec::new() }
    }

    /// Takes `wanted` to be what's followed now, letting go of what was held only through a workspace, or an issue or a
    /// pull request, no longer in it.
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
        let references = (wanted.iter())
            .filter_map(|follow| match follow.level() {
                Level::Reference { reference } => Some(reference),
                Level::Hub
                | Level::Workspace { .. }
                | Level::Session { .. }
                | Level::Terminal { .. }
                | Level::Unknown => None,
            })
            .collect::<Vec<_>>();
        let gone = std::mem::replace(&mut self.workspaces, now).into_iter().filter(|id| !self.workspaces.contains(id));
        let gone = gone.map(|id| Level::Workspace { id }).collect::<Vec<_>>();
        let unshown = std::mem::replace(&mut self.references, references);
        let unshown = unshown.into_iter().filter(|ask| !self.references.contains(ask));
        for level in gone.into_iter().chain(unshown.map(|reference| Level::Reference { reference })) {
            self.records.change(|records| records.let_go_of(&level));
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
        for id in self.workspaces.iter().filter(|id| !listed.contains(id)) {
            self.records.change(|records| records.let_go_of(&Level::Workspace { id: id.clone() }));
        }
        self.changing = Some(subscription.changing());
        Ok(subscription)
    }

    /// Takes what a workspace's level, or an issue's or a pull request's, sends as observations into the copy of the
    /// hub's records, telling the views here that show what GitHub says of it, and holds what's sent as
    /// `GithubUpdate` here, letting go of what the copy holds of it as an observation.
    fn apply(&mut self, _: &Client, held: &Mutex<GithubHeld>, item: SubscriptionItem) -> Result<bool> {
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
            SubscriptionItem::Github(update) => {
                if let GithubUpdate::Reference { reference, .. } = &*update {
                    let key = ObservationKey::Reference { reference: reference.clone() };
                    change(&|records| records.forget_observed(|of| *of == key));
                }
                Ok(held.lock().apply(*update))
            }
            SubscriptionItem::Listed(listed) => {
                let keys = match *listed {
                    Listed::Workspace(observations) => {
                        let keys = github_keys(&observations.observations);
                        change(&|records| records.apply_workspace_list(observations.clone()));
                        keys
                    }
                    Listed::Reference(observations) => {
                        let keys = github_keys(&observations.observations);
                        change(&|records| records.apply_reference_list(observations.clone()));
                        keys
                    }
                    // Of a level this doesn't follow.
                    Listed::Hub(_) | Listed::Session(_) | Listed::Unknown => Vec::new(),
                };
                held.lock().observed(&keys, changed.get());
                Ok(false)
            }
            SubscriptionItem::Notice(notice) => {
                let key = notice_key(&notice);
                change(&|records| records.apply_notice((*notice).clone()));
                if let Some(key) = key {
                    held.lock().observed(&[key], changed.get());
                }
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
