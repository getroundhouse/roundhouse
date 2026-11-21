//! Reading the feed as the hub starts and every hour after, and keeping what it found as a record every client follows.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::HubNotice;
use protocol::model::{HubUpdate, Rev};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::MissedTickBehavior;
use tracing::warn;

use super::reading::{UpdateFeed, read};
use crate::hub::Hub;
use crate::store::{Store, now_ms};

/// How long the hub waits to read a feed again whose signature didn't match, as one being published can be, as the
/// app waits.
const RETRY_AFTER: Duration = Duration::from_secs(5);

/// What the hub last found in the feed, which only a hub installed as a service reads.
#[derive(Default)]
pub(crate) struct Updates {
    /// Held while what was found changes and takes its revision, and while every record is listed, so a list of
    /// records has what was found as of the revision it takes in.
    changing: AsyncMutex<()>,
    found: Mutex<Option<HubUpdate>>,
}

impl Updates {
    /// Lists the records along with what was found, under the lock a change takes.
    pub(crate) async fn listed_with<T>(&self, list: impl Future<Output = T>) -> (T, Option<HubUpdate>) {
        let _changing = self.changing.lock().await;
        let listed = list.await;
        (listed, self.found.lock().clone())
    }
}

impl Hub {
    /// Reads `feed` as the hub starts, and every hour after while it serves, for the release a hub on `running`, a
    /// release's version, would update to. Each read tells every client, so a client knows when the hub last read it.
    pub(crate) async fn look_for_updates(self: Arc<Self>, feed: UpdateFeed, running: &'static str) {
        let mut every = tokio::time::interval(self.timers.look_for_updates_every);
        // Tokio's timers don't count the time a machine sleeps, so a hub that slept reads it an hour after it woke.
        every.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            every.tick().await;
            self.look_for_update(&feed, running, RETRY_AFTER).await;
        }
    }

    /// Reads `feed` once, and records what it found.
    pub(crate) async fn look_for_update(&self, feed: &UpdateFeed, running: &str, retry_after: Duration) {
        let (_, oldest) = self.workers_running();
        let (feed, running_now) = (feed.clone(), running.to_owned());
        let read = tokio::task::spawn_blocking(move || {
            let read = read(&feed, retry_after).map_err(|e| e.to_string())?;
            let update = read.hub_update(&running_now, oldest).map_err(|e| e.to_string())?;
            Ok::<_, String>(update.map(|release| release.version.to_owned()))
        })
        .await
        .unwrap_or_else(|e| Err(format!("reading the update feed stopped: {e}")));
        let _changing = self.updates.changing.lock().await;
        let rev = match self.blocking(Store::next_rev).await {
            Ok(rev) => rev,
            Err(e) => return warn!("recording what the update feed says: {}", e.message),
        };
        let update = self.updates.found(read, rev);
        self.notify(HubNotice::HubUpdate(update));
    }
}

impl Updates {
    /// Keeps what a read found, at `rev`. A read that failed leaves the version as the one before found it.
    fn found(&self, read: Result<Option<String>, String>, rev: Rev) -> HubUpdate {
        let mut found = self.found.lock();
        let before = found.as_ref().and_then(|update| update.version.clone());
        let (version, problem) = match read {
            Ok(version) => (version, None),
            Err(problem) => {
                warn!("reading the update feed: {problem}");
                (before, Some(problem))
            }
        };
        let update = HubUpdate { version, looked_at_ms: now_ms(), problem, rev };
        *found = Some(update.clone());
        update
    }
}
