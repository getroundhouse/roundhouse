use std::sync::Arc;

use parking_lot::Mutex;
use protocol::worker::HUB_SPEAKS;

/// What the app's update has to keep, as the app learns it from its hub, which the source reads the feed without each
/// release that wouldn't keep.
#[derive(Clone, Debug)]
pub(crate) struct ToKeep(Arc<Mutex<Keeping>>);

/// What [`ToKeep`] holds at one moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Keeping {
    /// The oldest version of the worker protocol among the workers the update will restart its hub with, which the
    /// next hub has to speak, so the update ends none of them. `None` when the restart leaves no worker running: on a
    /// hub someone runs themselves, which the app leaves alone, and on one whose workers stop with it, which the app
    /// restarts only once nothing is running.
    pub(crate) workers: Option<u32>,
    /// The release of this computer's hub, whose database the next hub migrates, so the update takes only a release
    /// that updates from it as well as from the app's own. The two differ while a hub the app doesn't restart at once
    /// is still on an older release. `None` until the app hears from it.
    pub(crate) hub: Option<String>,
}

impl ToKeep {
    /// Until the app has heard from its hub: workers as old as its own build's hub speaks, so the update crosses no break
    /// in the worker protocol. *Unconfirmed.*
    pub(crate) fn until_heard() -> Self {
        Self::oldest_at(Some(HUB_SPEAKS.oldest))
    }

    /// Workers as old as `oldest`, and no hub heard from.
    pub(crate) fn oldest_at(oldest: Option<u32>) -> Self {
        Self::keeping(Keeping { workers: oldest, hub: None })
    }

    pub(crate) fn keeping(keeping: Keeping) -> Self {
        Self(Arc::new(Mutex::new(keeping)))
    }

    pub(crate) fn now(&self) -> Keeping {
        self.0.lock().clone()
    }

    /// How that changes what the update has to keep.
    pub(crate) fn set(&self, keeping: Keeping) -> Kept {
        let mut was = self.0.lock();
        let older_workers = match (was.workers, keeping.workers) {
            (None, Some(_)) => true,
            (Some(was), Some(oldest)) => oldest < was,
            _ => false,
        };
        let kept = if *was == keeping {
            Kept::Same
        } else if older_workers || was.hub != keeping.hub {
            // A hub on another release may have a database that an update that kept the last one's can't migrate.
            Kept::Older
        } else {
            Kept::Newer
        };
        *was = keeping;
        kept
    }
}

/// How a change to what the update has to keep changes which updates keep it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kept {
    Same,
    /// Older workers, or some where there were none, or another hub, so an update that kept the last ones may not keep
    /// these.
    Older,
    /// Only newer workers, or none, so every update that kept the last ones keeps these, and a newer one may too.
    Newer,
}
