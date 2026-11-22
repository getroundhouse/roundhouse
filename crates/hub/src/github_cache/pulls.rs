use std::collections::HashMap;
use std::time::Duration;

use github::Checks;
use parking_lot::Mutex;
use protocol::model::{CommitId, History, Read, Stack};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tracing::warn;

use super::file::Row;
use super::kept::{KEPT_FOR, millis};
use super::keys::{Facet, Item, Subject};

/// What the cache keeps of each pull request's facets past its summary, as a read of pull requests found them, each
/// with the time it was read. A read of them that fails says why to whoever asked, so nothing's kept of why.
#[derive(Default)]
pub(super) struct KeptPulls(Mutex<HashMap<Item, Pull>>);

#[derive(Default)]
struct Pull {
    history: Option<Read<History>>,
    stack: Option<Read<Stack>>,
    /// The checks of the head they were last read at, which are another commit's than those its next head has.
    checks: Option<Read<Checks>>,
    asked_at_ms: i64,
}

/// A pull request's facets past its summary, as the cache has them.
pub(super) struct PastSummary {
    pub(super) history: Option<Read<History>>,
    pub(super) stack: Option<Read<Stack>>,
    pub(super) checks: Option<Read<Checks>>,
}

/// Whether `read` was read within `fresh_for` by `now_ms`.
fn fresh<T>(read: &Read<T>, fresh_for: Duration, now_ms: i64) -> bool {
    now_ms - read.read_at_ms < millis(fresh_for)
}

/// `facet` of `item`, `said`, read at `read_at_ms`, as the file keeps it.
fn row<T: Serialize>(item: &Item, facet: Facet, said: &T, read_at_ms: i64) -> Option<Row> {
    let said = serde_json::to_string(said)
        .inspect_err(|error| warn!("writing a pull request's facet for the cache's file: {error}"))
        .ok()?;
    Some(Row { subject: Subject::Item(item.clone()), facet, said, read_at_ms })
}

/// What the file keeps of a facet, read at `read_at_ms`, or `None` for what this build can't read.
fn read<T: DeserializeOwned>(said: &str, read_at_ms: i64) -> Option<Read<T>> {
    Some(Read { said: serde_json::from_str(said).ok()?, read_at_ms })
}

impl KeptPulls {
    /// Takes in `row`, as the file kept it, if it's a pull request's facet.
    pub(super) fn load(&self, row: &Row) {
        let Subject::Item(item) = &row.subject else { return };
        let mut kept = self.0.lock();
        let pull = kept.entry(item.clone()).or_default();
        pull.asked_at_ms = pull.asked_at_ms.max(row.read_at_ms);
        let (said, at) = (row.said.as_str(), row.read_at_ms);
        match row.facet {
            Facet::History => pull.history = read(said, at),
            Facet::Stack => pull.stack = read(said, at),
            Facet::Checks => pull.checks = read(said, at),
            Facet::Summary | Facet::Rules => {}
        }
        if pull.history.is_none() && pull.stack.is_none() && pull.checks.is_none() {
            kept.remove(item);
        }
    }

    /// `item`'s facets past its summary, where every one was read within `fresh_for` by `now_ms`, and its checks are of
    /// `head`, which counts as its being asked about. They're kept from one read, so they're of one moment.
    pub(super) fn fresh(&self, item: &Item, head: &CommitId, fresh_for: Duration, now_ms: i64) -> Option<PastSummary> {
        let mut kept = self.0.lock();
        let pull = kept.get_mut(item)?;
        pull.asked_at_ms = now_ms;
        let (history, stack, checks) = (pull.history.as_ref()?, pull.stack.as_ref()?, pull.checks.as_ref()?);
        let fresh =
            fresh(history, fresh_for, now_ms) && fresh(stack, fresh_for, now_ms) && fresh(checks, fresh_for, now_ms);
        (fresh && checks.said.head == *head).then(|| PastSummary {
            history: Some(history.clone()),
            stack: Some(stack.clone()),
            checks: Some(checks.clone()),
        })
    }

    /// `item`'s facets past its summary, however old, but for checks of another head than `head`, which counts as its
    /// being asked about.
    pub(super) fn known(&self, item: &Item, head: &CommitId, now_ms: i64) -> PastSummary {
        let mut kept = self.0.lock();
        let Some(pull) = kept.get_mut(item) else { return PastSummary { history: None, stack: None, checks: None } };
        pull.asked_at_ms = now_ms;
        PastSummary {
            history: pull.history.clone(),
            stack: pull.stack.clone(),
            checks: pull.checks.clone().filter(|checks| checks.said.head == *head),
        }
    }

    /// Keeps `read`, each pull request's facets past its summary as a read that began at the time beside them found
    /// them, but for those a read begun since has kept, and returns the rows the file keeps them as.
    pub(super) fn keep(&self, read: Vec<(Item, History, Stack, Checks, i64)>) -> Vec<Row> {
        let mut kept = self.0.lock();
        let mut rows = Vec::new();
        for (item, history, stack, checks, now_ms) in read {
            let newer =
                kept.get(&item).and_then(|pull| pull.history.as_ref()).is_some_and(|kept| kept.read_at_ms > now_ms);
            if newer {
                continue;
            }
            rows.extend(row(&item, Facet::History, &history, now_ms));
            rows.extend(row(&item, Facet::Stack, &stack, now_ms));
            rows.extend(row(&item, Facet::Checks, &checks, now_ms));
            let pull = Pull {
                history: Some(Read { said: history, read_at_ms: now_ms }),
                stack: Some(Read { said: stack, read_at_ms: now_ms }),
                checks: Some(Read { said: checks, read_at_ms: now_ms }),
                asked_at_ms: now_ms,
            };
            kept.insert(item, pull);
        }
        rows
    }

    /// Forgets what nothing has asked for in [`KEPT_FOR`] by `now_ms`, and returns it, as the file keeps it.
    pub(super) fn forget_unasked(&self, now_ms: i64) -> Vec<(Subject, Facet)> {
        let mut forgotten = Vec::new();
        self.0.lock().retain(|item, pull| {
            let keep = now_ms - pull.asked_at_ms < millis(KEPT_FOR);
            if !keep {
                let facets = [Facet::History, Facet::Stack, Facet::Checks];
                forgotten.extend(facets.map(|facet| (Subject::Item(item.clone()), facet)));
            }
            keep
        });
        forgotten
    }

    /// Makes what's kept as old as if `by` had gone by, for a test.
    #[cfg(test)]
    pub(super) fn age(&self, by: Duration) {
        for pull in self.0.lock().values_mut() {
            for read_at_ms in [
                pull.history.as_mut().map(|read| &mut read.read_at_ms),
                pull.stack.as_mut().map(|read| &mut read.read_at_ms),
                pull.checks.as_mut().map(|read| &mut read.read_at_ms),
            ]
            .into_iter()
            .flatten()
            {
                *read_at_ms -= millis(by);
            }
            pull.asked_at_ms -= millis(by);
        }
    }
}
