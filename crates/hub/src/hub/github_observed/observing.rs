//! The task that works out each observation of GitHub, and what it works each out from.

use std::collections::HashSet;
use std::sync::Arc;

use protocol::client::ApiError;
use protocol::model::{IssueSaid, PullRequestSaid, Reference, ReferenceAsk, ReferenceSaid};
use tokio::sync::broadcast::error::RecvError;
use tracing::warn;

use crate::github_cache::{Item, KeptSummary, Told};
use crate::hub::Hub;
use crate::hub::observations::Referenced;

/// What the task last saw clients follow.
#[derive(Default)]
struct Observing {
    followed: HashSet<ReferenceAsk>,
}

impl Hub {
    /// Works out each observation of GitHub whenever what it's worked out from changes, as `super` says, starting with
    /// removing those whose issue or pull request the cache no longer has, as after it forgot what was kept for too
    /// long while the hub was away. Runs until the hub stops.
    pub(crate) async fn observe_github(self: Arc<Self>) {
        // Before anything is worked out, so nothing that changes after is missed.
        let mut told = self.github_cache.changes();
        let mut followers = self.followers.changes();
        let mut observing = Observing::default();
        self.forget_unkept().await;
        observing.follow_now(&self).await;
        loop {
            tokio::select! {
                changed = followers.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    observing.follow_now(&self).await;
                }
                told_now = told.recv() => match told_now {
                    Ok(told_now) => {
                        // First, so a read kept just after something began to follow it is current, whichever the task
                        // hears of first.
                        observing.follow_now(&self).await;
                        observing.told(&self, told_now).await;
                    }
                    Err(RecvError::Lagged(_)) => {
                        self.forget_unkept().await;
                        observing.follow_now(&self).await;
                        observing.work_out_all(&self).await;
                    }
                    Err(RecvError::Closed) => return,
                },
            }
        }
    }

    /// Removes each observation of a chip whose issue or pull request the cache has nothing of.
    async fn forget_unkept(&self) {
        for ask in self.observations.kept_of::<Referenced>() {
            if self.github_cache.summary_kept(&item_of(&ask)).is_none() {
                logged(self.observation_gone::<Referenced>(ask).await);
            }
        }
    }

    /// Works out what GitHub says of `ask`, from what the cache keeps of it, and keeps it current from now if
    /// `current`, unless the cache's last read of it failed, after which it's as of the read before. Leaves it as it
    /// is where the cache has nothing of it, as before its first read.
    async fn work_out(&self, ask: &ReferenceAsk, current: bool) {
        let Some(KeptSummary { found, read_at_ms, failed_since }) = self.github_cache.summary_kept(&item_of(ask))
        else {
            return;
        };
        let Some(said) = said(found) else { return };
        let keeping = if current && !failed_since {
            self.observation_kept_current::<Referenced>(ask.clone()).await
        } else {
            self.observation_no_longer_read_again::<Referenced>(ask.clone()).await
        };
        logged(keeping);
        logged(self.observation_worked_out::<Referenced>(ask.clone(), said, read_at_ms).await);
    }
}

impl Observing {
    /// Takes in what clients follow now. What's newly followed is worked out at once from what the cache has, as of
    /// when that was read, and is current from the read following it starts. What's no longer followed is as of its
    /// last read.
    async fn follow_now(&mut self, hub: &Hub) {
        let now = hub.followers.reference_asks();
        for ask in now.difference(&self.followed) {
            hub.work_out(ask, false).await;
            logged(hub.observation_kept_current::<Referenced>(ask.clone()).await);
        }
        for ask in self.followed.difference(&now) {
            logged(hub.observation_no_longer_read_again::<Referenced>(ask.clone()).await);
        }
        self.followed = now;
    }

    /// Takes in what the cache kept and forgot: each observation worked out from a read it kept is worked out again,
    /// and each of what it forgot is gone.
    async fn told(&self, hub: &Hub, told: Told) {
        let Told { kept, forgotten } = told;
        for ask in self.asks(hub) {
            let item = item_of(&ask);
            if forgotten.contains(&item) {
                logged(hub.observation_gone::<Referenced>(ask).await);
            } else if kept.contains(&item) {
                hub.work_out(&ask, self.followed.contains(&ask)).await;
            }
        }
    }

    /// Works out every observation of a chip again, as after missing some of what the cache told.
    async fn work_out_all(&self, hub: &Hub) {
        for ask in self.asks(hub) {
            hub.work_out(&ask, self.followed.contains(&ask)).await;
        }
    }

    /// Every chip that's followed, or that the store has an observation of.
    fn asks(&self, hub: &Hub) -> HashSet<ReferenceAsk> {
        let mut asks = self.followed.clone();
        asks.extend(hub.observations.kept_of::<Referenced>());
        asks
    }
}

/// Says in the log why a change to an observation failed, which only a database that gives no revision fails, so it's
/// as it was until what it's worked out from changes again.
fn logged(changed: Result<(), ApiError>) {
    if let Err(error) = changed {
        warn!("keeping an observation of GitHub: {}", error.message);
    }
}

/// The issue or pull request `ask` names, as the cache keeps it.
fn item_of(ask: &ReferenceAsk) -> Item {
    Item::new(&ask.repository, ask.number)
}

/// What a chip is sent of what GitHub said, which `found` gives as `None` where there's none to be seen. `None` for one
/// of a kind this build doesn't know, which the hub never reads.
fn said(found: Option<Reference>) -> Option<ReferenceSaid> {
    Some(match found {
        None => ReferenceSaid::Missing,
        Some(Reference::Issue(issue)) => ReferenceSaid::Issue(IssueSaid {
            repository: issue.repository,
            number: issue.number,
            summary: issue.summary.said,
        }),
        Some(Reference::PullRequest(pull_request)) => ReferenceSaid::PullRequest(PullRequestSaid {
            repository: pull_request.repository,
            number: pull_request.number,
            summary: pull_request.summary.said,
        }),
        Some(Reference::Unknown) => return None,
    })
}
