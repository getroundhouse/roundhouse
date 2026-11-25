//! Reading the feed as the hub starts and every 10 minutes after, and keeping the release it offers, as an observation,
//! and when it was read and how that went, with how an update a client asked for is going, as a record every client
//! follows.

use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{HubUpdate, ReleaseOffer, Rev, UpdateAttempt};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::MissedTickBehavior;
use tracing::warn;
use update_feed::HubArchive;

use super::reading::{UpdateFeed, read};
use crate::hub::Hub;
use crate::hub::observations::ReleaseOffered;
use crate::store::{Store, now_ms};

/// How long the hub waits to read a feed again whose signature didn't match, as one being published can be, as the
/// app waits.
const RETRY_AFTER: Duration = Duration::from_secs(5);

/// What the hub last found in the feed, which only a hub installed as a service reads, and how an update a client
/// asked for is going.
pub(crate) struct Updates {
    /// Held while what was found changes and takes its revision, and while every record is listed, so a list of
    /// records has what was found as of the revision it takes in.
    changing: AsyncMutex<()>,
    found: Mutex<Found>,
    /// Where the hub reads the feed, which it's given before it serves anyone.
    source: OnceLock<Source>,
    /// Where the service keeps its builds, beside which an update places the next.
    pub(super) service_builds: PathBuf,
}

/// The feed a hub reads, and the release it runs, which what it finds there has to be newer than.
#[derive(Clone, Debug)]
pub(crate) struct Source {
    pub(super) feed: UpdateFeed,
    pub(super) running: &'static str,
}

/// What the hub's reads of the feed found, and how an update is going, each changed only by its own, and told of
/// together, as one record.
#[derive(Default)]
struct Found {
    /// What the last read found, once there's been one.
    look: Option<Look>,
    /// The update a client last asked for, which reads of the feed leave as it is.
    attempt: Option<UpdateAttempt>,
    rev: Rev,
}

/// How the hub's last read of the feed went.
struct Look {
    /// The archive of the release the hub would update to, by its version, which came under the feed's signature, as
    /// the observation [`ReleaseOffered`] says that release. `None` when none is newer.
    archive: Option<(String, HubArchive)>,
    looked_at_ms: i64,
    /// Why the read failed, which left `archive` as the read before found it.
    problem: Option<String>,
}

impl Found {
    /// The record every client follows, once the hub has read the feed. The release offered is the observation
    /// `ReleaseOffered`.
    fn record(&self) -> Option<HubUpdate> {
        let look = self.look.as_ref()?;
        Some(HubUpdate {
            looked_at_ms: look.looked_at_ms,
            problem: look.problem.clone(),
            attempt: self.attempt.clone(),
            rev: self.rev,
        })
    }
}

impl Updates {
    pub(crate) fn new(service_builds: PathBuf) -> Self {
        Self { changing: AsyncMutex::new(()), found: Mutex::default(), source: OnceLock::new(), service_builds }
    }

    /// Lists the records along with what was found, under the lock a change takes.
    pub(crate) async fn listed_with<T>(&self, list: impl Future<Output = T>) -> (T, Option<HubUpdate>) {
        let _changing = self.changing.lock().await;
        let listed = list.await;
        (listed, self.found.lock().record())
    }

    /// Says where the hub reads the feed, and which release it runs, before it serves anyone, so a client can have it
    /// update to a release found there as soon as it's found. Only the first is kept.
    pub(crate) fn read_from(&self, feed: UpdateFeed, running: &'static str) {
        let _ = self.source.set(Source { feed, running });
    }

    /// Where the hub reads the feed, if it does.
    pub(super) fn source(&self) -> Option<&Source> {
        self.source.get()
    }
}

impl Hub {
    /// Reads the feed it was given as the hub starts, and every 10 minutes after while it serves, for the release it would
    /// update to. Each read tells every client, so a client knows when the hub last read it.
    pub(crate) async fn look_for_updates(self: Arc<Self>) {
        let Some(source) = self.updates.source().cloned() else { return };
        let mut every = tokio::time::interval(self.timers.look_for_updates_every);
        // Tokio's timers don't count the time a machine sleeps, so a hub that slept reads it 10 minutes after it woke.
        every.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            every.tick().await;
            self.look_for_update(&source, RETRY_AFTER).await;
        }
    }

    /// Reads the feed once, and records what it found.
    pub(crate) async fn look_for_update(&self, source: &Source, retry_after: Duration) {
        let (_, oldest) = self.workers_running();
        let (feed, running) = (source.feed.clone(), source.running);
        let read = tokio::task::spawn_blocking(move || {
            let read = read(&feed, retry_after).map_err(|e| e.to_string())?;
            let update = read.hub_update(running, oldest).map_err(|e| e.to_string())?;
            // The release it picks always has an archive.
            Ok::<_, String>(update.and_then(|release| Some((release.version.to_owned(), release.hub?.clone()))))
        })
        .await
        .unwrap_or_else(|e| Err(format!("reading the update feed stopped: {e}")));
        self.release_offered(&read).await;
        let recorded = self
            .change_update(|found| {
                found.read(read);
                Ok(())
            })
            .await;
        if let Err(e) = recorded {
            warn!(name: "update.unrecorded", error = e.message, "Couldn't record what the update feed says");
        }
    }

    /// Keeps the release `read` found offered, current while the reads work, or as of the last that did, once one
    /// fails. Before the record says the read, so its copy holds what the read found.
    async fn release_offered(&self, read: &Result<Option<(String, HubArchive)>, String>) {
        let kept = match read {
            Ok(newer) => {
                let offered = ReleaseOffer { version: newer.as_ref().map(|(version, _)| version.clone()) };
                match self.observation_kept_current::<ReleaseOffered>(()).await {
                    Ok(()) => self.observation_read::<ReleaseOffered>((), offered, now_ms()).await,
                    Err(e) => Err(e),
                }
            }
            Err(_) => self.observation_no_longer_read_again::<ReleaseOffered>(()).await,
        };
        if let Err(e) = kept {
            warn!(name: "update.unkept", error = e.message, "Couldn't keep the release the update feed offers");
        }
    }

    /// Changes what the hub found, as `change` does, gives it the next revision, and tells every client. `change`
    /// refuses by giving an error, which changes nothing.
    pub(super) async fn change_update<T>(
        &self,
        change: impl FnOnce(&mut FoundNow<'_>) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        let _changing = self.updates.changing.lock().await;
        let rev = self.blocking(Store::next_rev).await?;
        let (outcome, record) = {
            let mut found = self.updates.found.lock();
            let outcome = change(&mut FoundNow { found: &mut found })?;
            found.rev = rev;
            (outcome, found.record())
        };
        // There's none to tell of only before the hub has read the feed, when no update can have started.
        if let Some(record) = record {
            self.notify(HubNotice::HubUpdate(record));
        }
        Ok(outcome)
    }
}

/// What the hub has found, while it's being changed.
pub(super) struct FoundNow<'a> {
    found: &'a mut Found,
}

/// What a client's request to update to a version comes to.
pub(super) enum Starting {
    /// The update to `version` starts, from `archive`.
    Starts { version: String, archive: HubArchive },
    /// It's already updating to that version, as a request sent again after its reply was lost finds.
    UnderWay,
}

impl FoundNow<'_> {
    /// Keeps what a read found: the archive of the release it would update to. A read that failed leaves that as the
    /// one before found it, and says why.
    fn read(&mut self, read: Result<Option<(String, HubArchive)>, String>) {
        let before = self.found.look.take().and_then(|look| look.archive);
        let (archive, problem) = match read {
            Ok(newer) => (newer, None),
            Err(problem) => {
                warn!(name: "update.feed_unread", error = problem, "Couldn't read the update feed");
                (before, Some(problem))
            }
        };
        self.found.look = Some(Look { archive, looked_at_ms: now_ms(), problem });
    }

    /// Starts updating to `version`, unless that isn't the release the hub would update to, as `offered` says, the
    /// observation of it, or it's updating to another.
    pub(super) fn start_updating(
        &mut self,
        version: &str,
        offered: Option<&ReleaseOffer>,
    ) -> Result<Starting, ApiError> {
        let refused = |why: String| ApiError::new(ErrorCode::Conflict, why);
        match &self.found.attempt {
            Some(UpdateAttempt::UnderWay { to }) if to == version => return Ok(Starting::UnderWay),
            Some(UpdateAttempt::UnderWay { to }) => {
                return Err(refused(format!("the hub is already updating to {to}")));
            }
            Some(UpdateAttempt::Failed { .. } | UpdateAttempt::Unknown) | None => {}
        }
        let found = match offered.map(|offered| offered.version.as_deref()) {
            None => return Err(refused("the hub hasn't read the update feed yet".to_owned())),
            Some(None) => {
                return Err(refused(format!("the hub has found no release newer than it, such as {version}")));
            }
            Some(Some(found)) if found != version => {
                return Err(refused(format!("the hub would update to {found}, not {version}")));
            }
            Some(Some(found)) => found,
        };
        // Kept just after the release it's of, as the read that found both is recorded.
        let of_it = self.found.look.as_ref().and_then(|look| look.archive.clone()).filter(|(of, _)| of == found);
        let Some((found, archive)) = of_it else {
            return Err(refused("the hub is still taking in what the update feed offers".to_owned()));
        };
        self.found.attempt = Some(UpdateAttempt::UnderWay { to: found.clone() });
        Ok(Starting::Starts { version: found, archive })
    }

    /// Says the update under way failed, and why, which leaves the release the hub would update to as it was, to try
    /// again.
    pub(super) fn failed(&mut self, why: String) {
        warn!(name: "update.failed", error = why, "Couldn't update the hub");
        if let Some(UpdateAttempt::UnderWay { to }) = self.found.attempt.take() {
            self.found.attempt = Some(UpdateAttempt::Failed { to, why });
        }
    }
}
