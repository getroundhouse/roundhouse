//! Which workspaces' lines are being read, so each is read once at a time.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::model::WorkspaceId;

use crate::worker_link::WorkerLink;

/// The workspaces whose lines are being read, so a workspace is read once at a time, however often something says its
/// lines may have changed meanwhile, and read once more after a read that something said so during, since that read
/// may have begun before the change.
#[derive(Default)]
pub(crate) struct Reads {
    /// Each workspace being read, with the worker to read it from again once that read is done, if something has said
    /// since it began that its lines may have changed: the worker the newest of them named.
    under: Arc<Mutex<Under>>,
}

/// Each workspace being read, as [`Reads`] keeps them.
type Under = HashMap<WorkspaceId, Option<Arc<WorkerLink>>>;

/// A workspace's turn to be read, which ends as it's dropped, so a read that's stopped partway leaves the next to begin.
pub(super) struct Reading {
    under: Arc<Mutex<Under>>,
    id: WorkspaceId,
    /// Whether [`Reading::again`] has ended it already, after which another read may have begun.
    ended: bool,
}

impl Reads {
    /// The workspace's turn to be read, unless a read of it is under way, which then reads it once more from `link`
    /// once it's done.
    pub(super) fn begin(&self, id: &WorkspaceId, link: &Arc<WorkerLink>) -> Option<Reading> {
        match self.under.lock().entry(id.clone()) {
            Entry::Occupied(mut under_way) => {
                under_way.insert(Some(link.clone()));
                None
            }
            Entry::Vacant(free) => {
                free.insert(None);
                Some(Reading { under: self.under.clone(), id: id.clone(), ended: false })
            }
        }
    }

    /// Whether a read of the workspace is under way, for a test to wait until it's done.
    #[cfg(test)]
    pub(super) fn under_way(&self, id: &WorkspaceId) -> bool {
        self.under.lock().contains_key(id)
    }
}

impl Reading {
    pub(super) fn id(&self) -> &WorkspaceId {
        &self.id
    }

    /// The worker to read the workspace from once more, if something said during the read that its lines may have
    /// changed, and otherwise `None`, which ends the turn.
    pub(super) fn again(&mut self) -> Option<Arc<WorkerLink>> {
        let mut reading = self.under.lock();
        let again = reading.get_mut(&self.id).and_then(Option::take);
        if again.is_none() {
            reading.remove(&self.id);
            self.ended = true;
        }
        again
    }
}

impl Drop for Reading {
    fn drop(&mut self) {
        if !self.ended {
            self.under.lock().remove(&self.id);
        }
    }
}
