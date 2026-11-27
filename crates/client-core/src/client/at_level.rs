//! Following each of a set of things at its own level, such as the workspaces a handle is kept on or the chips views
//! show, through one subscription, which changes what it follows in one request as the set changes, and taking what
//! each sends into the copy of the hub's records.

use parking_lot::Mutex;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::Follow;
use protocol::model::Level;

use crate::client::follow::{Reach, Source};
use crate::client::subscribing::Changing;
use crate::client::{Connection, Result, Subscription, SubscriptionItem};
use crate::records::HubRecords;

/// One of what a [`LevelSource`] follows, each at its own level: a workspace, or an issue or a pull request a chip
/// shows.
pub(in crate::client) trait AtLevel: Clone + PartialEq + 'static {
    /// What says which are followed, which what follows them keeps.
    type Wanted: Send + 'static;

    /// Whether the hub lists each before it answers the request that follows it, as it does a workspace, so one with
    /// no list once that's answered is one it refused, and what was held of it from the connection before is let go of.
    /// Where the list may come after the answer, as a chip's may, one with no list yet may only be waiting for it, so
    /// what was held of it is kept.
    const LISTED_BEFORE_ANSWER: bool;

    /// Each that `wanted` says is followed, once.
    fn wanted(wanted: &Self::Wanted) -> Vec<Self>;

    /// What follows it.
    fn follow(self) -> Follow;

    /// The level it's followed at.
    fn level(self) -> Level;

    /// The one `level` is, if it's of this kind.
    fn at(level: Level) -> Option<Self>;

    /// Takes `listed`, a list of what's at one of its level, into `records`, if it's of this kind. Returns whether that
    /// changed them.
    fn take_list(records: &mut HubRecords, listed: Listed) -> bool;
}

/// What changes what the subscription on this connection follows, and where what it sends goes. What's held through
/// one is let go of once it isn't followed.
pub(in crate::client) struct LevelSource<L> {
    records: Reach<HubRecords>,
    changing: Option<Changing>,
    /// What was last followed on this connection, which isn't asked for again until it changes, so a start the hub
    /// refused isn't sent again each time what's wanted is said again unchanged. A start the hub refused, such as of a
    /// workspace it no longer has, is left out until what's wanted changes, or a new connection, and what was held of
    /// it is kept or let go of as [`AtLevel::LISTED_BEFORE_ANSWER`] says.
    followed: Vec<L>,
}

impl<L: AtLevel> LevelSource<L> {
    /// Writes what's sent in `records`, the copy of the same hub's records.
    pub(in crate::client) fn new(records: Reach<HubRecords>) -> Self {
        LevelSource { records, changing: None, followed: Vec::new() }
    }

    /// Takes `wanted` to be what's followed now, letting go of what was held only through one no longer in it.
    fn followed_now(&mut self, wanted: Vec<L>) {
        let before = std::mem::replace(&mut self.followed, wanted);
        for gone in before.into_iter().filter(|one| !self.followed.contains(one)) {
            self.records.change(|records| records.let_go_of(&gone.level()));
        }
    }

    /// What follows each of `wanted`.
    fn follows(wanted: &[L]) -> Vec<Follow> {
        wanted.iter().cloned().map(L::follow).collect()
    }
}

impl<L: AtLevel> Source for LevelSource<L> {
    type Held = L::Wanted;

    /// Where the hub lists each before it answers, what's held of one followed with no list once it's answered, from
    /// the connection before, is let go of, as [`AtLevel::LISTED_BEFORE_ANSWER`] says.
    fn resume(&mut self, connection: &Connection, held: &Mutex<L::Wanted>) -> Result<Subscription> {
        self.changing = None;
        let wanted = L::wanted(&held.lock());
        let (subscription, _refused) = connection.follow_all(Self::follows(&wanted))?;
        self.followed_now(wanted);
        if L::LISTED_BEFORE_ANSWER {
            let mut listed = Vec::new();
            for item in subscription.items.try_iter() {
                if let SubscriptionItem::Listed(_) = &item
                    && let Some(one) = item.level().and_then(L::at)
                {
                    listed.push(one);
                }
                self.apply(connection, held, item)?;
            }
            for one in self.followed.iter().filter(|one| !listed.contains(one)) {
                self.records.change(|records| records.let_go_of(&one.clone().level()));
            }
        }
        self.changing = Some(subscription.changing());
        Ok(subscription)
    }

    /// Takes what one's level sends into the copy of the hub's records, which tells its own readers when that changes
    /// it. What's sent of one no longer followed, which was already on its way as it was let go of, is dropped, so
    /// nothing brings back what letting go of it let go of.
    fn apply(&mut self, _: &Connection, _: &Mutex<L::Wanted>, item: SubscriptionItem) -> Result<bool> {
        let Some(one) = item.level().and_then(L::at) else { return Ok(false) };
        if !self.followed.contains(&one) {
            return Ok(false);
        }
        match item {
            SubscriptionItem::Listed(listed) => {
                self.records.change(|records| L::take_list(records, *listed));
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
        // What's held here is what's wanted, which nothing the hub sends changes.
        Ok(false)
    }

    /// Follows each wanted now, if that has changed, and lets go of the rest.
    fn asked(&mut self, connection: Option<&Connection>, held: &Mutex<L::Wanted>, _: &dyn Fn()) -> Result<bool> {
        let wanted = L::wanted(&held.lock());
        let (Some(connection), Some(changing)) = (connection, &self.changing) else { return Ok(false) };
        if wanted == self.followed {
            return Ok(false);
        }
        changing.follow_only(connection, Self::follows(&wanted))?;
        self.followed_now(wanted);
        Ok(false)
    }

    fn reconnected(&mut self) {
        self.changing = None;
    }
}
