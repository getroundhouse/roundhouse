use std::sync::Arc;

use parking_lot::Mutex;
use protocol::worker::HUB_SPEAKS;

/// The oldest version of the worker protocol among the workers the app's update will restart its hub with, which the
/// next hub has to speak, so the update ends none of them. `None` when the restart leaves no worker running: on a hub
/// someone runs themselves, which the app leaves alone, and on one whose workers stop with it, which the app restarts
/// only once nothing is running. The app says it as it learns it from its hub, and the source reads the feed without
/// each release whose hub doesn't speak it.
#[derive(Clone, Debug)]
pub(crate) struct WorkersToKeep(Arc<Mutex<Option<u32>>>);

impl WorkersToKeep {
    /// Until the app has heard from its hub: workers as old as its own build's hub speaks, so the update crosses no break
    /// in the worker protocol. *Unconfirmed.*
    pub(crate) fn until_heard() -> Self {
        Self::oldest_at(Some(HUB_SPEAKS.oldest))
    }

    pub(crate) fn oldest_at(oldest: Option<u32>) -> Self {
        Self(Arc::new(Mutex::new(oldest)))
    }

    pub(crate) fn oldest(&self) -> Option<u32> {
        *self.0.lock()
    }

    /// How that changes what the update has to keep.
    pub(crate) fn set(&self, oldest: Option<u32>) -> Kept {
        let was = std::mem::replace(&mut *self.0.lock(), oldest);
        match (was, oldest) {
            _ if was == oldest => Kept::Same,
            (None, Some(_)) => Kept::Older,
            (Some(was), Some(oldest)) if oldest < was => Kept::Older,
            _ => Kept::Newer,
        }
    }
}

/// How a change to the workers to keep changes which updates keep them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kept {
    Same,
    /// Older workers, or some where there were none, so an update that kept the last ones may not keep these.
    Older,
    /// Only newer ones, or none, so every update that kept the last ones keeps these, and a newer one may too.
    Newer,
}
