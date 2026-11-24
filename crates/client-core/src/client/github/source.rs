//! Following each issue or pull request a chip shows, which the hub then reads, all through one subscription, which
//! changes what it follows in one request as what's shown changes. What the hub says of every workspace's own pull
//! request comes from following the hub, as [`crate::RecordsFeed`] does, and what it says only at a workspace's own
//! level from following the workspace there, as `client::workspace_level` does. What a chip's level sends as an
//! observation goes into the copy of the hub's records.

use parking_lot::Mutex;
use std::cell::Cell;

use protocol::client::listed::Listed;
use protocol::client::subscriptions::Follow;
use protocol::model::{Level, ReferenceAsk};

use super::held::{GithubHeld, github_keys, notice_key};
use crate::client::follow::{Reach, Source};
use crate::client::subscribing::Changing;
use crate::client::{Client, Result, Subscription, SubscriptionItem};
use crate::records::HubRecords;

/// What changes what the subscription on this connection follows. What a chip's level sends goes into the copy of the
/// hub's records, and what's held through it is let go of once it isn't followed.
pub(super) struct GithubSource {
    /// The copy of the same hub's records, which the GitHub feed reads.
    records: Reach<HubRecords>,
    changing: Option<Changing>,
    /// What was last asked for on this connection, which isn't asked for again until it changes, so a start the hub
    /// refused isn't sent again each time a view says what it shows.
    asked_for: Vec<Follow>,
    /// The issues and pull requests followed at their own level.
    references: Vec<ReferenceAsk>,
}

impl GithubSource {
    pub(super) fn new(records: Reach<HubRecords>) -> Self {
        GithubSource { records, changing: None, asked_for: Vec::new(), references: Vec::new() }
    }

    /// Takes `wanted` to be what's followed now, letting go of what was held only through an issue or a pull request no
    /// longer in it.
    /// A start the hub refused is left out until what's shown next changes, or a new connection, and the views show it
    /// as not known.
    fn followed_now(&mut self, wanted: &[Follow]) {
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
        let unshown = std::mem::replace(&mut self.references, references);
        let unshown = unshown.into_iter().filter(|ask| !self.references.contains(ask)).collect::<Vec<_>>();
        for reference in unshown {
            self.records.change(|records| records.let_go_of(&Level::Reference { reference }));
        }
    }
}

impl Source for GithubSource {
    type Held = GithubHeld;

    fn resume(&mut self, client: &Client, held: &Mutex<GithubHeld>) -> Result<Subscription> {
        self.changing = None;
        let wanted = followed(&held.lock());
        let (subscription, _refused) = client.follow_all(wanted.clone())?;
        self.followed_now(&wanted);
        self.asked_for.clone_from(&wanted);
        self.changing = Some(subscription.changing());
        Ok(subscription)
    }

    /// Takes what an issue's or a pull request's level sends as observations into the copy of the hub's records, telling
    /// the views here that show it. What's sent of a chip no longer followed, which was already on its way as it was let go
    /// of, is dropped, so nothing brings back what letting go of it let go of.
    fn apply(&mut self, _: &Client, held: &Mutex<GithubHeld>, item: SubscriptionItem) -> Result<bool> {
        let Some(Level::Reference { reference }) = item.level() else { return Ok(false) };
        if !self.references.contains(&reference) {
            return Ok(false);
        }
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
            SubscriptionItem::Listed(listed) => {
                let keys = match *listed {
                    Listed::Reference(observations) => {
                        let keys = github_keys(&observations.observations);
                        change(&|records| records.apply_reference_list(observations.clone()));
                        keys
                    }
                    // Of a level this doesn't follow.
                    Listed::Hub(_) | Listed::Workspace(_) | Listed::Session(_) | Listed::Unknown => Vec::new(),
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

    /// Follows the chips shown now, if that has changed, and lets go of what no view shows.
    fn asked(&mut self, connection: Option<&Client>, held: &Mutex<GithubHeld>, _: &dyn Fn()) -> Result<bool> {
        let wanted = followed(&held.lock());
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

/// What to follow for the chips the views show.
fn followed(held: &GithubHeld) -> Vec<Follow> {
    held.shown().references.into_iter().map(|reference| Follow::Reference { reference }).collect()
}
