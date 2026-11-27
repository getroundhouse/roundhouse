//! What a hub observes, as a client reads it: each observation of a level the client follows, as the level last said
//! it.

use std::collections::HashMap;
use std::time::Instant;

use client_core::{Connection, Subscription, SubscriptionItem};
use protocol::client::HubNotice;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::Follow;
use protocol::model::{Branches, Observation, ObservationKey, Observed, WorkspaceId};

use super::stack::TIMEOUT;

/// A level a client follows, and each of its observations as it last said it.
pub(crate) struct Observing {
    followed: Subscription,
    held: HashMap<ObservationKey, Observation>,
}

impl Observing {
    /// Follows `level`, which lists its observations at once.
    pub(crate) fn at(client: &Connection, level: Follow) -> Self {
        Self { followed: client.follow(level).unwrap(), held: HashMap::new() }
    }

    /// What `pick` picks out of the level's observations, once `holds` is true of it, as the level tells of each
    /// change. Fails the test after [`TIMEOUT`].
    pub(crate) fn until<T>(
        &mut self,
        what: &str,
        pick: impl Fn(&Observation) -> Option<T>,
        holds: impl Fn(Option<&T>) -> bool,
    ) -> Option<T> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let now = self.held.values().find_map(&pick);
            if holds(now.as_ref()) {
                return now;
            }
            let left = deadline.checked_duration_since(Instant::now()).unwrap_or_else(|| panic!("waiting for {what}"));
            let item = self.followed.items.recv_timeout(left).unwrap_or_else(|_| panic!("waiting for {what}"));
            self.take(item);
        }
    }

    /// Keeps what `item` says of the level's observations.
    fn take(&mut self, item: SubscriptionItem) {
        match item {
            SubscriptionItem::Listed(listed) => {
                let observations = match *listed {
                    Listed::Hub(records) => records.observations,
                    Listed::Workspace(listed) => listed.observations,
                    Listed::Session(records) => records.observations,
                    Listed::Reference(listed) => listed.observations,
                    Listed::Unknown => Vec::new(),
                };
                self.held = observations.into_iter().map(|observation| (observation.key(), observation)).collect();
            }
            SubscriptionItem::Notice(notice) => match *notice {
                HubNotice::Observation(observation) => {
                    self.held.insert(observation.key(), observation);
                }
                HubNotice::ObservationRemoved { observation, .. } => {
                    self.held.remove(&observation);
                }
                _ => {}
            },
            _ => {}
        }
    }
}

/// What `observed` says, whether current or as of a read, or `None` while it isn't known.
pub(crate) fn known<T>(observed: &Observed<T>) -> Option<&T> {
    match observed {
        Observed::Current { value } | Observed::AsOf { value, .. } => Some(value),
        Observed::NotKnown | Observed::Unknown => None,
    }
}

/// The branches of the workspace `id`, as its level says them, once `holds` is true of them.
pub(crate) fn branches_once(
    client: &Connection,
    id: &WorkspaceId,
    what: &str,
    holds: impl Fn(Option<&Branches>) -> bool,
) -> Option<Branches> {
    let branches = |observation: &Observation| match observation {
        Observation::WorkspaceBranches { workspace_id, branches, .. } if workspace_id == id => known(branches).cloned(),
        _ => None,
    };
    Observing::at(client, Follow::Workspace { id: id.clone() }).until(what, branches, holds)
}

/// What the hub's level says, as `pick` picks it out, which a client reads in the list of every record.
pub(crate) fn at_the_hub<T>(client: &Connection, pick: impl Fn(&Observation) -> Option<T>) -> Option<T> {
    client.records().unwrap().observations.iter().find_map(pick)
}
