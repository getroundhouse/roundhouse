use std::collections::HashMap;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::model::{GithubRepositoryName, IssueNumber, Reference, ReferenceRead};
use tokio::sync::watch;
use tokio::time::Instant;

/// How long what the hub found of one stays as it was before a client's ask reads it again, so one on screen in a
/// client that asks every minute says it's merged within a minute or two of it merging. A read that failed is tried
/// again no sooner, so a GitHub that's down or slow is asked no more often than one that answers. *Unconfirmed.*
pub(super) const FRESH_FOR: Duration = Duration::from_secs(60);

/// How long the hub keeps what it found of one nobody has asked about since, so it doesn't keep everything any chat
/// ever named.
const KEPT_FOR: Duration = Duration::from_hours(1);

/// An issue or a pull request, by its repository, as GitHub writes it in lower case, since GitHub's names ignore case,
/// and its number.
pub(super) type Key = (String, IssueNumber);

pub(super) fn key(repository: &GithubRepositoryName, number: IssueNumber) -> Key {
    (repository.as_str().to_ascii_lowercase(), number)
}

/// What the hub found of each issue and pull request it's been asked about, and which are being read from GitHub now.
#[derive(Default)]
pub(in crate::hub) struct Kept {
    found: Mutex<HashMap<Key, Found>>,
    /// Those being read from GitHub now, each with what tells an ask that wants it too, such as a second client
    /// showing the same chat, that the read is over, however it ended, so the ask waits for its answer rather than
    /// asking GitHub again, and waits for nothing else.
    reading: Mutex<HashMap<Key, watch::Receiver<()>>>,
}

struct Found {
    /// What GitHub last said, or `None` when it hasn't said anything yet.
    said: Option<Said>,
    /// Why the last read failed, if it did, which is kept beside what GitHub said before.
    failed: Option<String>,
    /// When it was last read, or tried.
    read_at: Instant,
    asked_at: Instant,
}

/// What GitHub said of an issue or a pull request.
enum Said {
    Found(Reference),
    /// There's no such issue or pull request that can be seen.
    Missing,
}

/// The reads of some of what was asked for that one ask makes itself, while it makes them. Dropping it, as the ask
/// ends, however it ends, tells those waiting for them that they're over.
pub(super) struct Reading<'a> {
    kept: &'a Kept,
    keys: Vec<Key>,
    _over: watch::Sender<()>,
}

impl Reading<'_> {
    pub(super) fn keys(&self) -> &[Key] {
        &self.keys
    }
}

impl Drop for Reading<'_> {
    fn drop(&mut self) {
        let mut reading = self.kept.reading.lock();
        for key in &self.keys {
            reading.remove(key);
        }
    }
}

impl Kept {
    /// Which of `keys` aren't fresh, each once: those this ask is to read itself, which no other is reading, and the
    /// reads by other asks of the rest, to wait for. Those read, or tried, within [`FRESH_FOR`] are neither.
    pub(super) fn claim(&self, keys: &[Key]) -> (Option<Reading<'_>>, Vec<watch::Receiver<()>>) {
        let found = self.found.lock();
        let mut reading = self.reading.lock();
        let (over, waiting) = watch::channel(());
        let (mut mine, mut theirs) = (Vec::new(), Vec::new());
        for key in keys {
            if found.get(key).is_some_and(|found| found.read_at.elapsed() < FRESH_FOR) || mine.contains(key) {
                continue;
            }
            if let Some(read) = reading.get(key) {
                theirs.push(read.clone());
            } else {
                reading.insert(key.clone(), waiting.clone());
                mine.push(key.clone());
            }
        }
        let mine = (!mine.is_empty()).then(|| Reading { kept: self, keys: mine, _over: over });
        (mine, theirs)
    }

    /// Keeps what GitHub said of `key` just now: what it found, or that there's none to be seen, or why it couldn't be
    /// read, beside what it said before.
    pub(super) fn keep(&self, key: Key, said: Result<Option<Reference>, String>) {
        let now = Instant::now();
        let mut found = self.found.lock();
        found.retain(|_, found| found.asked_at.elapsed() < KEPT_FOR);
        let before = found.remove(&key).and_then(|found| found.said);
        let (said, failed) = match said {
            Ok(Some(reference)) => (Some(Said::Found(reference)), None),
            Ok(None) => (Some(Said::Missing), None),
            Err(why) => (before, Some(why)),
        };
        found.insert(key, Found { said, failed, read_at: now, asked_at: now });
    }

    /// Makes what's kept as old as if `by` had gone by since it was read, for a test of what's read again.
    #[cfg(test)]
    pub(super) fn age(&self, by: Duration) {
        for found in self.found.lock().values_mut() {
            found.read_at = found.read_at.checked_sub(by).unwrap_or(found.read_at);
        }
    }

    /// What's known of `key`, which counts as its being asked about: what GitHub last said of it, or, if it's said
    /// nothing, why it couldn't be read. `None` if it was never read.
    pub(super) fn read(&self, key: &Key) -> Option<ReferenceRead> {
        let mut found = self.found.lock();
        let found = found.get_mut(key)?;
        found.asked_at = Instant::now();
        Some(match (&found.said, &found.failed) {
            (Some(Said::Found(reference)), _) => ReferenceRead::Found { reference: Box::new(reference.clone()) },
            (Some(Said::Missing), _) => ReferenceRead::Missing,
            (None, Some(why)) => ReferenceRead::Unread { message: why.clone() },
            (None, None) => return None,
        })
    }
}
