use std::sync::Arc;

use parking_lot::Mutex;
use protocol::worker::HUB_SPEAKS;
use update_feed::{HubRelease, Running};

/// What the app's update has to keep, as the app learns it from its hub, which the source reads the feed without each
/// release that wouldn't keep.
#[derive(Clone, Debug)]
pub(crate) struct ToKeep(Arc<Mutex<Keeping>>);

/// What [`ToKeep`] holds at one moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Keeping {
    /// As [`Heard::workers`] says.
    workers: Option<u32>,
    hub: Hub,
}

/// Which release the update takes this computer's hub to be on. Its database is from that release, and the next hub
/// migrates it, so the update takes only a release that updates from it as well as from the app's own. The two differ
/// while a hub the app doesn't restart at once is still on an older release.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Hub {
    NotHeard,
    /// On the release it said in its status.
    Heard(String),
    /// It failed to start, or couldn't be reached, so the update goes by the app's own release, `app`, alone, so that a
    /// release that fixes a broken hub still arrives. Should the hub's database be older than the release's floor, that
    /// release's hub refuses it and leaves it untouched, as `docs/spec/updates.md` says.
    FailedToStart {
        app: String,
    },
}

impl Hub {
    fn release(&self) -> HubRelease<&str> {
        match self {
            Hub::NotHeard => HubRelease::NotHeard,
            Hub::Heard(release) | Hub::FailedToStart { app: release } => HubRelease::On(release),
        }
    }
}

/// What the app has heard from its hub that the update has to keep.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Heard {
    /// The oldest version of the worker protocol among the workers the update will restart its hub with, which the
    /// next hub has to speak, so the update ends none of them. `None` when the restart leaves no worker running: on a
    /// hub someone runs themselves, which the app leaves alone, and on one whose workers stop with it, which the app
    /// restarts only once nothing is running.
    pub(crate) workers: Option<u32>,
    /// The hub's release.
    pub(crate) hub: String,
}

impl Keeping {
    /// Where an update from the app's release, `version`, goes from.
    pub(super) fn running<'a>(&'a self, version: &'a str) -> Running<'a> {
        Running { version, hub: self.hub.release(), workers: self.workers }
    }
}

impl ToKeep {
    /// Until the app has heard from its hub: workers as old as its own build's hub speaks, so the update crosses no break
    /// in the worker protocol, and no hub release, so it takes no release that says the oldest release it updates from.
    /// *Unconfirmed.*
    pub(crate) fn until_heard() -> Self {
        Self::oldest_at(Some(HUB_SPEAKS.oldest))
    }

    /// Workers as old as `oldest`, and no hub heard from.
    pub(crate) fn oldest_at(oldest: Option<u32>) -> Self {
        Self(Arc::new(Mutex::new(Keeping { workers: oldest, hub: Hub::NotHeard })))
    }

    pub(crate) fn now(&self) -> Keeping {
        self.0.lock().clone()
    }

    /// How hearing that changes what the update has to keep.
    pub(crate) fn set(&self, heard: Heard) -> Kept {
        self.change(|_| Keeping { workers: heard.workers, hub: Hub::Heard(heard.hub) })
    }

    /// How this computer's hub failing to start changes what the update has to keep, for an app on release `app`. The
    /// workers stay as they were, since the hub may have left some running.
    pub(crate) fn failed_to_start(&self, app: &str) -> Kept {
        self.change(|was| Keeping { workers: was.workers, hub: Hub::FailedToStart { app: app.to_owned() } })
    }

    fn change(&self, to: impl FnOnce(&Keeping) -> Keeping) -> Kept {
        let mut was = self.0.lock();
        let keeping = to(&was);
        let older_workers = match (was.workers, keeping.workers) {
            (None, Some(_)) => true,
            (Some(was), Some(oldest)) => oldest < was,
            _ => false,
        };
        // A hub on an older release than before may have a database that the update it took can't migrate. What the
        // update took before the app heard from its hub updates from any release, so any hub's database takes it.
        let older_hub = !keeping.hub.release().past_every_floor_of(&was.hub.release());
        let kept = if *was == keeping {
            Kept::Same
        } else if older_workers || older_hub {
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
    /// Older workers, or some where there were none, or a hub on an older release, or one that isn't a release's, so
    /// an update that kept the last ones may not keep these.
    Older,
    /// Only newer workers, or none, and a hub on the same release or a newer one, or the first hub heard from, so every
    /// update that kept the last ones keeps these, and a newer one may too.
    Newer,
}
