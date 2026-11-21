use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::model::{Reference, ReferenceRead};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tracing::warn;

use super::file::{CacheFile, Row};
use super::keys::{Facet, Item};
use crate::store::now_ms;

/// How long the cache keeps what nothing has asked for since, in memory and in its file, so it doesn't keep everything
/// any chat ever named, but still has what was shown before a restart, or a weekend away. *Unconfirmed.*
pub(super) const KEPT_FOR: Duration = Duration::from_hours(24 * 7);

/// What the hub last read from GitHub of each issue and pull request it's been asked about, and which facets of which
/// are being read now.
pub(crate) struct GithubCache {
    items: Mutex<HashMap<Item, Kept>>,
    /// Those being read from GitHub now, each with what tells an ask that wants it too, such as a second client
    /// showing the same chat, that the read is over, however it ended, so the ask waits for its answer rather than
    /// asking GitHub again, and waits for nothing else.
    reading: Mutex<HashMap<(Item, Facet), watch::Receiver<()>>>,
    file: CacheFile,
}

/// What's kept of one issue or pull request.
struct Kept {
    summary: LastRead,
    asked_at_ms: i64,
}

/// What the last read of a facet found.
struct LastRead {
    /// What GitHub last said, or `None` when it hasn't said anything yet.
    said: Option<Said>,
    /// Why the last read failed, if it did, which is kept beside what GitHub said before.
    failed: Option<String>,
    /// When it was last read, or tried.
    read_at_ms: i64,
}

/// What GitHub said of an issue's or a pull request's summary, as the cache's file keeps it too.
#[derive(Serialize, Deserialize)]
#[serde(tag = "found", rename_all = "snake_case")]
enum Said {
    Found {
        reference: Box<Reference>,
    },
    /// There's no such issue or pull request that can be seen.
    Missing,
}

impl From<Option<Reference>> for Said {
    fn from(found: Option<Reference>) -> Self {
        found.map_or(Self::Missing, |reference| Self::Found { reference: Box::new(reference) })
    }
}

impl Said {
    /// It, as the cache's file keeps `item`'s summary, read at `read_at_ms`.
    fn row(&self, item: &Item, read_at_ms: i64) -> Option<Row> {
        let said = serde_json::to_string(self)
            .inspect_err(|error| warn!("writing what GitHub said for the cache's file: {error}"))
            .ok()?;
        Some(Row { item: item.clone(), facet: Facet::Summary, said, read_at_ms })
    }
}

/// The reads of a facet of some of what was asked for that one ask makes itself, while it makes them. Dropping it, as
/// the ask ends, however it ends, tells those waiting for them that they're over.
pub(crate) struct Reading<'a> {
    cache: &'a GithubCache,
    facet: Facet,
    items: Vec<Item>,
    _over: watch::Sender<()>,
}

impl Reading<'_> {
    pub(crate) fn items(&self) -> &[Item] {
        &self.items
    }
}

impl Drop for Reading<'_> {
    fn drop(&mut self) {
        let mut reading = self.cache.reading.lock();
        for item in self.items.drain(..) {
            reading.remove(&(item, self.facet));
        }
    }
}

impl GithubCache {
    /// The cache kept in the file at `path`, with what it kept from before that's still to be kept.
    pub(crate) fn open(path: &Path) -> Self {
        let file = CacheFile::open(path);
        let since_ms = now_ms().saturating_sub(millis(KEPT_FOR));
        let rows = file.load(since_ms).unwrap_or_else(|error| {
            warn!("reading what was last read from GitHub from {}: {error}", path.display());
            Vec::new()
        });
        let mut items = HashMap::new();
        for Row { item, facet, said, read_at_ms } in rows {
            let Facet::Summary = facet;
            let Ok(said) = serde_json::from_str::<Said>(&said) else { continue };
            let summary = LastRead { said: Some(said), failed: None, read_at_ms };
            items.insert(item, Kept { summary, asked_at_ms: read_at_ms });
        }
        Self { items: Mutex::new(items), reading: Mutex::new(HashMap::new()), file }
    }

    /// Which of `items` have a `facet` older than `fresh_for`, each once: those this ask is to read itself, which no
    /// other is reading, and the reads by other asks of the rest, to wait for. Those read, or tried, within
    /// `fresh_for` are neither.
    pub(crate) fn claim(
        &self,
        items: &[Item],
        facet: Facet,
        fresh_for: Duration,
    ) -> (Option<Reading<'_>>, Vec<watch::Receiver<()>>) {
        let now_ms = now_ms();
        let kept = self.items.lock();
        let mut reading = self.reading.lock();
        let (over, waiting) = watch::channel(());
        let (mut mine, mut theirs) = (Vec::new(), Vec::new());
        for item in items {
            let fresh = kept.get(item).is_some_and(|kept| now_ms - kept.summary.read_at_ms < millis(fresh_for));
            if fresh || mine.contains(item) {
                continue;
            }
            if let Some(read) = reading.get(&(item.clone(), facet)) {
                theirs.push(read.clone());
            } else {
                reading.insert((item.clone(), facet), waiting.clone());
                mine.push(item.clone());
            }
        }
        let mine = (!mine.is_empty()).then(|| Reading { cache: self, facet, items: mine, _over: over });
        (mine, theirs)
    }

    /// Keeps what GitHub said of each summary just now: what it found, or that there's none to be seen, or why it
    /// couldn't be read, beside what it said before. Writes what it found to the file, and forgets what nothing has
    /// asked for in [`KEPT_FOR`]. Blocks on the file.
    pub(crate) fn keep(&self, read: Vec<(Item, Result<Option<Reference>, String>)>) {
        let read_at_ms = now_ms();
        let forgotten = self.forget_unasked(read_at_ms);
        let mut rows = Vec::new();
        {
            let mut items = self.items.lock();
            for (item, said) in read {
                let summary = match said {
                    Ok(found) => {
                        let said = Said::from(found);
                        rows.extend(said.row(&item, read_at_ms));
                        LastRead { said: Some(said), failed: None, read_at_ms }
                    }
                    Err(why) => {
                        let before = items.remove(&item).and_then(|kept| kept.summary.said);
                        LastRead { said: before, failed: Some(why), read_at_ms }
                    }
                };
                items.insert(item, Kept { summary, asked_at_ms: read_at_ms });
            }
        }
        let written = self.file.forget(&forgotten).and_then(|()| rows.iter().try_for_each(|row| self.file.write(row)));
        if let Err(error) = written {
            warn!("keeping what was read from GitHub in the cache's file: {error}");
        }
    }

    /// Forgets, in memory, what nothing has asked for in [`KEPT_FOR`] by `now_ms`, and returns it.
    fn forget_unasked(&self, now_ms: i64) -> Vec<Item> {
        let mut forgotten = Vec::new();
        self.items.lock().retain(|item, kept| {
            let keep = now_ms - kept.asked_at_ms < millis(KEPT_FOR);
            if !keep {
                forgotten.push(item.clone());
            }
            keep
        });
        forgotten
    }

    /// What's known of `item`'s summary, which counts as its being asked about: what GitHub last said of it, or, if
    /// it's said nothing, why it couldn't be read. `None` if it was never read.
    pub(crate) fn summary(&self, item: &Item) -> Option<ReferenceRead> {
        let mut items = self.items.lock();
        let kept = items.get_mut(item)?;
        kept.asked_at_ms = now_ms();
        Some(match (&kept.summary.said, &kept.summary.failed) {
            (Some(Said::Found { reference }), _) => ReferenceRead::Found { reference: reference.clone() },
            (Some(Said::Missing), _) => ReferenceRead::Missing,
            (None, Some(why)) => ReferenceRead::Unread { message: why.clone() },
            (None, None) => return None,
        })
    }

    /// Makes what's kept as old as if `by` had gone by since it was read and asked about, for a test of what's read
    /// again and what's forgotten.
    #[cfg(test)]
    pub(crate) fn age(&self, by: Duration) {
        for kept in self.items.lock().values_mut() {
            kept.summary.read_at_ms -= millis(by);
            kept.asked_at_ms -= millis(by);
        }
    }
}

/// `duration` in milliseconds, or the most there are.
fn millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}
