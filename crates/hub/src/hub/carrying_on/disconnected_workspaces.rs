//! The workspaces whose worker the hub can't reach, which it waits for: since when and why, how their providers have
//! been asked whether the worker is gone, and what waits for either to change.

use crate::worker_link::WorkerLinkId;
use parking_lot::Mutex;
use protocol::model::{InstanceId, WorkspaceId};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::Instant;

/// Why a workspace's worker can't be reached.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cause {
    /// A hub that was running left it, and this hub, as it started, found it hadn't dialed.
    LeftByEarlierHub,
    /// Its connection to this hub ended with no goodbye.
    ConnectionDropped,
}

/// What a provider said when it was asked whether a disconnected workspace's worker is gone, and it wasn't proved to be.
#[derive(Clone, Copy)]
pub(crate) enum Asked {
    /// It answered, or didn't in time, or failed, so it's to be asked again.
    Said,
    /// It can't say, as most adapters can't, so it isn't asked again.
    CannotSay,
}

/// When a disconnected workspace's provider is next to be asked whether its worker is gone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum NextAsk {
    Now,
    In(Duration),
    Never,
}

/// How often a provider is asked about a worker that has just been disconnected, as the time disconnected is small, and
/// the fewest between two asks. A worker that has just died can still read as running for a moment, since the system
/// lets go of its claim only as the last of its threads exits.
const ASK_AT_LEAST_EVERY: Duration = Duration::from_millis(250);
/// The most between two asks, for a worker that's been disconnected a long while, which an adapter that can be asked
/// costs a request each of.
const ASK_AT_MOST_EVERY: Duration = Duration::from_secs(30);

struct Entry {
    since: Instant,
    cause: Cause,
    instance_id: InstanceId,
    /// Whether the provider's first answer, or that it can't tell, has come: until it has, nothing says the worker isn't
    /// gone, so a request for a worker waits for it rather than being refused.
    settled: bool,
    /// When the provider was last asked, or that it can't be.
    asked: Option<Instant>,
    cant_say: bool,
    /// Its worker's connection ended again while it was being carried on with, so the wait isn't over.
    dropped_again: bool,
    /// The links whose connections dropped, which the agents of its sessions were on, so each is told they're lost if
    /// the worker is found gone.
    links: Vec<WorkerLinkId>,
}

/// The workspaces whose worker the hub can't reach, which it waits for, and since when. A worker's connection is how it
/// talks to its hub, not proof that it's alive, so a workspace is here from when a connection ends with no goodbye, or
/// a starting hub finds that a worker an earlier hub left hasn't dialed, until the worker dials, its provider says it's gone, or the
/// hub stops. It also holds how the provider has been asked whether the worker is gone, so how often it's asked
/// doesn't depend on how often the wait looks again. Only in memory, since it's the hub that's running that can't
/// reach the worker, and the next works that out again.
#[derive(Default)]
pub(crate) struct Disconnected {
    entries: Mutex<HashMap<WorkspaceId, Entry>>,
    /// Told each time a workspace leaves or its provider has answered, for what waits on either.
    changed: Notify,
}

impl Disconnected {
    /// Says the workspace's worker can't be reached, from now, unless it already was. `settled` is whether its provider
    /// has been asked already, and didn't say it's gone. Whether it newly is.
    pub(crate) fn mark(
        &self,
        id: &WorkspaceId,
        instance_id: &InstanceId,
        cause: Cause,
        settled: bool,
        dropped: Option<WorkerLinkId>,
    ) -> bool {
        let mut entries = self.entries.lock();
        if entries.contains_key(id) {
            return false;
        }
        let instance_id = instance_id.clone();
        let entry = Entry {
            since: Instant::now(),
            cause,
            instance_id,
            settled,
            asked: None,
            cant_say: false,
            dropped_again: false,
            links: dropped.into_iter().collect(),
        };
        entries.insert(id.clone(), entry);
        true
    }

    /// Says a look for the worker is about to begin, so a drop that's told of from now is of what it takes up.
    pub(crate) fn begin_attempt(&self, id: &WorkspaceId) {
        if let Some(entry) = self.entries.lock().get_mut(id) {
            entry.dropped_again = false;
        }
    }

    /// Says its worker's connection ended again while it was being carried on with, so the workspace isn't connected
    /// when that's done.
    pub(crate) fn dropped_again(&self, id: &WorkspaceId, link: WorkerLinkId) {
        if let Some(entry) = self.entries.lock().get_mut(id) {
            entry.dropped_again = true;
            entry.links.push(link);
        }
    }

    /// Whether its worker was connected to this hub until its connection dropped, which a worker found gone was, so
    /// its agents were lost with it, where one an earlier hub left was never this hub's to lose.
    pub(crate) fn dropped(&self, id: &WorkspaceId) -> bool {
        self.entries.lock().get(id).is_some_and(|entry| entry.cause == Cause::ConnectionDropped)
    }

    /// Says the workspace is connected, or recovered, unless its worker's connection has ended again since it was last
    /// taken up. Whether it did.
    pub(crate) fn clear_unless_dropped_again(&self, id: &WorkspaceId) -> bool {
        let mut entries = self.entries.lock();
        if entries.get_mut(id).is_some_and(|entry| std::mem::take(&mut entry.dropped_again)) {
            return false;
        }
        entries.remove(id);
        self.changed.notify_waiters();
        true
    }

    /// The links of the workspace's whose connections dropped.
    pub(crate) fn dropped_links(&self, id: &WorkspaceId) -> Vec<WorkerLinkId> {
        self.entries.lock().get(id).map_or_else(Vec::new, |entry| entry.links.clone())
    }

    pub(crate) fn clear(&self, id: &WorkspaceId) {
        self.entries.lock().remove(id);
        self.changed.notify_waiters();
    }

    pub(crate) fn contains(&self, id: &WorkspaceId) -> bool {
        self.entries.lock().contains_key(id)
    }

    pub(crate) fn ids(&self) -> Vec<WorkspaceId> {
        self.entries.lock().keys().cloned().collect()
    }

    pub(crate) fn any(&self) -> bool {
        !self.entries.lock().is_empty()
    }

    /// When the provider is next to be asked whether the worker on `instance_id` is gone, if its workspace is
    /// disconnected: at once if it hasn't been, then as seldom as the time since the workspace was disconnected says,
    /// an eighth of it, between a quarter of a second and half a minute, and never once it said it can't tell.
    pub(crate) fn next_ask(&self, instance_id: &InstanceId) -> Option<NextAsk> {
        let entries = self.entries.lock();
        let entry = entries.values().find(|entry| entry.instance_id == *instance_id)?;
        if entry.cant_say {
            return Some(NextAsk::Never);
        }
        let Some(last) = entry.asked else { return Some(NextAsk::Now) };
        let pace = (entry.since.elapsed() / 8).clamp(ASK_AT_LEAST_EVERY, ASK_AT_MOST_EVERY);
        let wait = pace.saturating_sub(last.elapsed());
        Some(if wait.is_zero() { NextAsk::Now } else { NextAsk::In(wait) })
    }

    /// Says the provider was asked about the worker on `instance_id` just now, and how it answered.
    pub(crate) fn asked(&self, instance_id: &InstanceId, how: Asked) {
        {
            let mut entries = self.entries.lock();
            let Some(entry) = entries.values_mut().find(|entry| entry.instance_id == *instance_id) else { return };
            entry.asked = Some(Instant::now());
            entry.settled = true;
            entry.cant_say |= matches!(how, Asked::CannotSay);
        }
        self.changed.notify_waiters();
    }

    /// Returns once the workspace isn't disconnected, which is at once if it isn't.
    pub(crate) async fn until_connected(&self, id: &WorkspaceId) {
        self.until(|entries| !entries.contains_key(id)).await;
    }

    /// Returns once the workspace isn't disconnected, or its provider has said that its worker isn't gone, or can't
    /// tell, which is at once if either is so. Until then nothing says whether it's to be waited for or replaced.
    pub(crate) async fn until_settled(&self, id: &WorkspaceId) {
        self.until(|entries| entries.get(id).is_none_or(|entry| entry.settled)).await;
    }

    async fn until(&self, done: impl Fn(&HashMap<WorkspaceId, Entry>) -> bool) {
        loop {
            // Before looking, so a change in between is seen.
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if done(&self.entries.lock()) {
                return;
            }
            changed.await;
        }
    }
}
