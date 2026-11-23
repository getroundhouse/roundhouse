//! What each workspace's worker was last told of watching its worktree, kept for each workspace only while there's
//! something to keep.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::model::WorkspaceId;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use crate::worker_link::WorkerLinkId;

/// What each workspace's worker was last told of watching, by workspace, each under a lock of its own, so what's told
/// goes one at a time and what's told last is what's so once followers stop coming and going. A workspace whose worker
/// watches nothing, and that nothing is telling, has no entry, so one that's gone leaves none.
#[derive(Default)]
pub(crate) struct Watches(Mutex<HashMap<WorkspaceId, Entry>>);

type Entry = Arc<AsyncMutex<Option<Told>>>;

/// What one worker was told, or answered. `None` in its place says it's watching nothing, as a worker the hub has just
/// taken on is, and one that has gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Told {
    pub(super) link: WorkerLinkId,
    /// What it was told to watch against, so a workspace whose base changes is told again.
    pub(super) base_ref: Option<String>,
    /// It refused, as a worker from before the request does, so it's asked no more.
    pub(super) refused: bool,
}

impl Watches {
    /// The workspace's turn to tell its worker, once any telling before it is done. What's told is kept as the turn
    /// ends.
    pub(super) async fn turn(&self, id: &WorkspaceId) -> Turn<'_> {
        let entry = self.0.lock().entry(id.clone()).or_default().clone();
        Turn { watches: self, id: id.clone(), told: entry.lock_owned().await }
    }

    /// How many workspaces have an entry, for a test to see that none is kept for nothing.
    #[cfg(test)]
    pub(super) fn kept(&self) -> usize {
        self.0.lock().len()
    }
}

/// A workspace's turn to tell its worker whether to watch, which derefs to what it was last told.
pub(super) struct Turn<'a> {
    watches: &'a Watches,
    id: WorkspaceId,
    told: OwnedMutexGuard<Option<Told>>,
}

impl Deref for Turn<'_> {
    type Target = Option<Told>;

    fn deref(&self) -> &Self::Target {
        &self.told
    }
}

impl DerefMut for Turn<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.told
    }
}

impl Drop for Turn<'_> {
    /// Drops the workspace's entry if its worker watches nothing and no other turn is waiting for it. Taking a turn
    /// clones the entry under the map's lock, which this holds, so none can be waiting unseen.
    fn drop(&mut self) {
        if self.told.is_some() {
            return;
        }
        let mut map = self.watches.0.lock();
        // The map's and this turn's.
        if map.get(&self.id).is_some_and(|entry| Arc::strong_count(entry) == 2) {
            map.remove(&self.id);
        }
    }
}
