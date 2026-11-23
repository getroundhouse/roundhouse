//! What each workspace's worker was last told of watching its worktree, kept for each workspace only while there's
//! something to keep.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::model::WorkspaceId;
use tokio::sync::OwnedMutexGuard;

use crate::hub::tellings::Tellings;
use crate::worker_link::WorkerLinkId;

/// What each workspace's worker was last told of watching, by workspace, each with tellings of its own, which go one at
/// a time, with at most one waiting for its turn, so what's told last is what's so once followers stop coming and going.
/// A workspace whose worker watches nothing, and that nothing is telling, has no entry, so one that's gone leaves none.
#[derive(Default)]
pub(crate) struct Watches(Mutex<HashMap<WorkspaceId, Entry>>);

type Entry = Arc<Tellings<Option<Told>>>;

/// A telling asked for, which is to wait for its turn, and holds its workspace's entry until it has it.
pub(super) struct Asked(Entry);

/// What one worker was told, or answered. `None` in its place says it's watching nothing, as a worker the hub has just
/// taken on is, and one that has gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Told {
    pub(super) link: WorkerLinkId,
    /// What it was told to watch against, so a workspace whose base changes is told again.
    pub(super) base_ref: Option<String>,
    pub(super) answer: Answer,
}

/// What a worker told to watch answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Answer {
    /// That it watches.
    Watching,
    /// That it can't, as a worker from before the request does, so it's asked no more.
    Refused,
    /// Nothing, in the time it's given. The request reached it all the same, so it can be watching, and is asked again
    /// while the workspace is followed, and told to stop once it isn't.
    Late,
}

impl Watches {
    /// The workspace's turn to tell its worker, once any telling before it is done. What's told is kept as the turn
    /// ends.
    pub(super) async fn turn(&self, id: &WorkspaceId) -> Turn<'_> {
        let entry = self.0.lock().entry(id.clone()).or_default().clone();
        self.turn_of(id, entry).await
    }

    /// A telling of the workspace's worker asked for, to have its turn on a task of its own, unless one is waiting for
    /// its turn already, which tells what this would, as [`Tellings`] says.
    pub(super) fn asked(&self, id: &WorkspaceId) -> Option<Asked> {
        let mut map = self.0.lock();
        let entry = map.entry(id.clone()).or_default();
        entry.asked().then(|| Asked(entry.clone()))
    }

    /// The turn of a telling asked for, as [`Watches::turn`] gives it.
    pub(super) async fn asked_turn(&self, id: &WorkspaceId, asked: Asked) -> Turn<'_> {
        self.turn_of(id, asked.0).await
    }

    async fn turn_of(&self, id: &WorkspaceId, entry: Entry) -> Turn<'_> {
        let told = entry.turn().await;
        Turn { watches: self, id: id.clone(), entry, told }
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
    entry: Entry,
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
    /// Drops the workspace's entry if its worker watches nothing and no other turn is waiting for it. Taking a turn, or
    /// asking for one, clones the entry under the map's lock, which this holds, so none can be waiting unseen.
    fn drop(&mut self) {
        if self.told.is_some() {
            return;
        }
        let mut map = self.watches.0.lock();
        // The map's and this turn's.
        if Arc::strong_count(&self.entry) == 2 {
            map.remove(&self.id);
        }
    }
}
