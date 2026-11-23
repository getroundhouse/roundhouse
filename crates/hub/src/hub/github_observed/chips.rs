//! What GitHub says of each issue and pull request a chip follows, worked out from what the cache keeps of its summary.

use std::collections::HashSet;

use protocol::model::{ReferenceAsk, ReferenceSaid};

use super::observing::logged;
use crate::github_cache::{Item, KeptSummary, Told};
use crate::hub::Hub;
use crate::hub::observations::Referenced;

/// The chips the task last saw clients follow.
#[derive(Default)]
pub(super) struct Chips {
    followed: HashSet<ReferenceAsk>,
}

impl Hub {
    /// Removes each observation of a chip whose issue or pull request the cache has nothing of.
    pub(super) async fn forget_unkept_chips(&self) {
        for ask in self.observations.kept_of::<Referenced>() {
            if self.github_cache.summary_kept(&item_of(&ask)).is_none() {
                logged(self.observation_gone::<Referenced>(ask).await);
            }
        }
    }

    /// Works out what GitHub says of `ask`, from what the cache keeps of it, and keeps it current from now if
    /// `current`, unless the cache's last read of it failed, after which it's as of the read before. Leaves it as it
    /// is where the cache has nothing of it, as before its first read.
    async fn work_out_chip(&self, ask: &ReferenceAsk, current: bool) {
        let Some(KeptSummary { found, read_at_ms, failed_since }) = self.github_cache.summary_kept(&item_of(ask))
        else {
            return;
        };
        let Some(said) = ReferenceSaid::of(found.as_ref()) else { return };
        let keeping = if current && !failed_since {
            self.observation_kept_current::<Referenced>(ask.clone()).await
        } else {
            self.observation_no_longer_read_again::<Referenced>(ask.clone()).await
        };
        logged(keeping);
        logged(self.observation_worked_out::<Referenced>(ask.clone(), said, read_at_ms).await);
    }
}

impl Chips {
    /// Takes in what clients follow now. What's newly followed is worked out at once from what the cache has, as of
    /// when that was read, and is current from the read following it starts. What's no longer followed is as of its
    /// last read.
    pub(super) async fn follow_now(&mut self, hub: &Hub) {
        let now = hub.followers.reference_asks();
        for ask in now.difference(&self.followed) {
            hub.work_out_chip(ask, false).await;
            logged(hub.observation_kept_current::<Referenced>(ask.clone()).await);
        }
        for ask in self.followed.difference(&now) {
            logged(hub.observation_no_longer_read_again::<Referenced>(ask.clone()).await);
        }
        self.followed = now;
    }

    /// Takes in what the cache kept and forgot: each observation worked out from a read it kept is worked out again,
    /// and each of what it forgot is gone.
    pub(super) async fn told(&self, hub: &Hub, told: &Told) {
        let Told { kept, forgotten } = told;
        for ask in self.asks(hub) {
            let item = item_of(&ask);
            if forgotten.contains(&item) {
                logged(hub.observation_gone::<Referenced>(ask).await);
            } else if kept.contains(&item) {
                hub.work_out_chip(&ask, self.followed.contains(&ask)).await;
            }
        }
    }

    /// Works out every observation of a chip again, as after missing some of what the cache told.
    pub(super) async fn work_out_all(&self, hub: &Hub) {
        for ask in self.asks(hub) {
            hub.work_out_chip(&ask, self.followed.contains(&ask)).await;
        }
    }

    /// Every chip that's followed, or that the store has an observation of.
    fn asks(&self, hub: &Hub) -> HashSet<ReferenceAsk> {
        let mut asks = self.followed.clone();
        asks.extend(hub.observations.kept_of::<Referenced>());
        asks
    }
}

/// The issue or pull request `ask` names, as the cache keeps it.
pub(super) fn item_of(ask: &ReferenceAsk) -> Item {
    Item::new(&ask.repository, ask.number)
}
