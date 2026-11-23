use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::model::{PrState, PullRequest, Read, Reference, ReferenceRead};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, watch};
use tracing::warn;

use github::{Checks, FacetsRead, PullFacets, Rules};
use protocol::model::{BranchName, GithubRepositoryName, IssueNumber};

use super::file::{GithubFile, Row};
use super::keys::{BranchOf, Facet, Item, Subject};
use super::pulls::KeptPulls;
use super::rules::KeptRules;
use super::told::Told;
use crate::cache_file::CacheFile;
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
    rules: KeptRules,
    pulls: KeptPulls,
    file: GithubFile,
    /// Tells of each read kept, and of what's forgotten, by the issues and pull requests they were of.
    pub(super) told: broadcast::Sender<Told>,
}

/// What the cache keeps of an issue's or a pull request's summary, as [`GithubCache::summary_kept`] gives it.
pub(crate) struct KeptSummary {
    /// What GitHub said of it, or `None` where it said there's none to be seen.
    pub(crate) found: Option<Reference>,
    /// When the read that found it began.
    pub(crate) read_at_ms: i64,
    /// Whether the read after that failed, so it's what was known before.
    pub(crate) failed_since: bool,
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
    /// When the read that found what GitHub said began.
    said_at_ms: i64,
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
        Some(Row { subject: Subject::Item(item.clone()), facet: Facet::Summary, said, read_at_ms })
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
    /// The cache kept in `file`, whose tables [`prepare`](super::prepare) made, with what it kept from before that's
    /// still to be kept.
    pub(crate) fn open(file: Arc<CacheFile>) -> Self {
        let file = GithubFile::new(file);
        let since_ms = now_ms().saturating_sub(millis(KEPT_FOR));
        let rows = file.load(since_ms).unwrap_or_else(|error| {
            warn!("reading what was last read from GitHub from the cache's file: {error}");
            Vec::new()
        });
        let (mut items, rules, pulls) = (HashMap::new(), KeptRules::default(), KeptPulls::default());
        for row in rows {
            rules.load(&row);
            pulls.load(&row);
            let Row { subject: Subject::Item(item), facet: Facet::Summary, said, read_at_ms } = row else { continue };
            let Ok(said) = serde_json::from_str::<Said>(&said) else { continue };
            let summary = LastRead { said: Some(said), failed: None, read_at_ms, said_at_ms: read_at_ms };
            items.insert(item, Kept { summary, asked_at_ms: read_at_ms });
        }
        let told = broadcast::Sender::new(super::told::KEPT_TOLD);
        Self { items: Mutex::new(items), reading: Mutex::new(HashMap::new()), rules, pulls, file, told }
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

    /// Keeps what GitHub said of each summary to a read that began at `read_at_ms`: what it found, or that there's none
    /// to be seen, or why it couldn't be read, beside what it said before. One that a read begun since has kept is left
    /// as that read found it, however late this one lands. Writes what it found to the file, and forgets what nothing
    /// has asked for in [`KEPT_FOR`]. Blocks on the file.
    pub(crate) fn keep(&self, read: Vec<(Item, Result<Option<Reference>, String>)>, read_at_ms: i64) {
        let now_ms = now_ms();
        let forgotten = self.forget_unasked(now_ms);
        let mut rows = Vec::new();
        let mut told = Vec::new();
        {
            let mut items = self.items.lock();
            for (item, said) in read {
                if items.get(&item).is_some_and(|kept| kept.summary.read_at_ms > read_at_ms) {
                    continue;
                }
                told.push(item.clone());
                let summary = match said {
                    Ok(found) => {
                        let said = Said::from(found);
                        rows.extend(said.row(&item, read_at_ms));
                        LastRead { said: Some(said), failed: None, read_at_ms, said_at_ms: read_at_ms }
                    }
                    Err(why) => {
                        let before = items.remove(&item).map(|kept| kept.summary);
                        let said_at_ms = before.as_ref().map_or(read_at_ms, |before| before.said_at_ms);
                        LastRead {
                            said: before.and_then(|before| before.said),
                            failed: Some(why),
                            read_at_ms,
                            said_at_ms,
                        }
                    }
                };
                items.insert(item, Kept { summary, asked_at_ms: now_ms });
            }
        }
        self.write(&forgotten, &rows);
        self.tell(told, &forgotten);
    }

    /// Forgets `forgotten` in the file, and writes `rows` to it, at once.
    fn write(&self, forgotten: &[(Subject, Facet)], rows: &[Row]) {
        if let Err(error) = self.file.keep(forgotten, rows) {
            warn!("keeping what was read from GitHub in the cache's file: {error}");
        }
    }

    /// `item` as a pull request, where each of its facets was read within `fresh_for` and its checks are of the head
    /// its summary is at, with them judged by the rules of the branch it merges into that were read within
    /// `rules_fresh_for`, which counts as its being asked about. `None` where any facet is older, or isn't kept, or
    /// there are no such rules for an open one's checks, which can't be judged without them.
    pub(crate) fn fresh_pull_request(
        &self,
        item: &Item,
        fresh_for: Duration,
        rules_fresh_for: Duration,
    ) -> Option<PullRequest> {
        let now_ms = now_ms();
        let pull_request = self.pull_request_summary(item, now_ms)?;
        if now_ms - pull_request.summary.read_at_ms >= millis(fresh_for) {
            return None;
        }
        let past = self.pulls.fresh(item, &pull_request.said().head, fresh_for, now_ms)?;
        let pull_request = PullRequest { history: past.history, stack: past.stack, ..pull_request };
        let rules = self.rules_into(&pull_request, rules_fresh_for);
        // A merged or closed one has no checks to judge, so it needs none.
        if rules.is_none() && pull_request.said().state == PrState::Open {
            return None;
        }
        Some(judged(pull_request, past.checks?, rules.as_ref()))
    }

    /// `item` as a pull request, in whichever facets the cache has however old, but for checks of another head than
    /// its summary is at, which it leaves out, so a push isn't taken for the commit before it. Its checks are judged by
    /// the rules of the branch it merges into, however old, or without, so which are required isn't known. Counts as its
    /// being asked about. `None` where it has no summary of it as a pull request.
    pub(crate) fn pull_request(&self, item: &Item) -> Option<PullRequest> {
        let now_ms = now_ms();
        let pull_request = self.pull_request_summary(item, now_ms)?;
        let past = self.pulls.known(item, &pull_request.said().head, now_ms);
        let pull_request = PullRequest { history: past.history, stack: past.stack, ..pull_request };
        let Some(checks) = past.checks else { return Some(pull_request) };
        let rules = self.rules_into(&pull_request, Duration::MAX);
        Some(judged(pull_request, checks, rules.as_ref()))
    }

    /// The rules of the branch `pull_request` merges into, as its stack says, read within `fresh_for`.
    fn rules_into(&self, pull_request: &PullRequest, fresh_for: Duration) -> Option<Rules> {
        let destination = &pull_request.stack_said()?.destination;
        (self.rules_of(&pull_request.repository, fresh_for).into_iter())
            .find(|(branch, _)| branch == destination)
            .map(|(_, rules)| rules)
    }

    /// `item` as a pull request with its summary alone, if the cache has that, which counts as its being asked about.
    fn pull_request_summary(&self, item: &Item, now_ms: i64) -> Option<PullRequest> {
        let mut items = self.items.lock();
        let kept = items.get_mut(item)?;
        kept.asked_at_ms = now_ms;
        let Some(Said::Found { reference }) = &kept.summary.said else { return None };
        let Reference::PullRequest(pull_request) = &**reference else { return None };
        Some(pull_request.clone())
    }

    /// Keeps the facets past its summary of each pull request a read of them found, as of when the read began, but for
    /// one a read begun since has kept, and forgets what nothing has asked for in [`KEPT_FOR`]. Its summary is kept
    /// apart, as a read of summaries keeps it. Blocks on the file.
    pub(crate) fn keep_pulls(&self, read: Vec<FacetsRead>) {
        let forgotten = self.forget_unasked(now_ms());
        let read: Vec<_> = (read.into_iter())
            .filter_map(|FacetsRead { repository, number, facets, read_at_ms }| {
                let number = IssueNumber::try_from(number.get()).ok()?;
                let PullFacets { history, stack, checks, .. } = facets;
                Some((Item::new(&repository, number), history, stack, checks, read_at_ms))
            })
            .collect();
        let told = read.iter().map(|(item, ..)| item.clone()).collect();
        let rows = self.pulls.keep(read);
        self.write(&forgotten, &rows);
        self.tell(told, &forgotten);
    }

    /// The rules of the branches of `repository` the cache has read within `fresh_for`, for a read of pull requests
    /// into them not to read again.
    pub(crate) fn rules_of(&self, repository: &GithubRepositoryName, fresh_for: Duration) -> Vec<(BranchName, Rules)> {
        self.rules.known(repository, fresh_for, now_ms())
    }

    /// Keeps the rules of each branch a read of pull requests that began at `read_at_ms` read, as `(repository, branch,
    /// rules)`, but for those a read begun since has kept, and forgets what nothing has asked for in [`KEPT_FOR`].
    /// Blocks on the file.
    pub(crate) fn keep_rules(&self, read: Vec<(GithubRepositoryName, BranchName, Rules)>, read_at_ms: i64) {
        let now_ms = now_ms();
        let forgotten = self.forget_unasked(now_ms);
        let read = read.into_iter().map(|(repository, branch, rules)| (BranchOf::new(&repository, branch), rules));
        let rows = self.rules.keep(read.collect(), read_at_ms, now_ms);
        self.write(&forgotten, &rows);
        self.tell(Vec::new(), &forgotten);
    }

    /// Forgets, in memory, what nothing has asked for in [`KEPT_FOR`] by `now_ms`, and returns it.
    fn forget_unasked(&self, now_ms: i64) -> Vec<(Subject, Facet)> {
        let mut forgotten = self.rules.forget_unasked(now_ms);
        forgotten.extend(self.pulls.forget_unasked(now_ms));
        self.items.lock().retain(|item, kept| {
            let keep = now_ms - kept.asked_at_ms < millis(KEPT_FOR);
            if !keep {
                forgotten.push((Subject::Item(item.clone()), Facet::Summary));
            }
            keep
        });
        forgotten
    }

    /// What the cache keeps of `item`'s summary, if GitHub has said anything of it, which doesn't count as its being
    /// asked about, so what works out what's sent of it from this keeps nothing longer than [`KEPT_FOR`].
    pub(crate) fn summary_kept(&self, item: &Item) -> Option<KeptSummary> {
        let items = self.items.lock();
        let summary = &items.get(item)?.summary;
        let found = match summary.said.as_ref()? {
            Said::Found { reference } => Some((**reference).clone()),
            Said::Missing => None,
        };
        Some(KeptSummary { found, read_at_ms: summary.said_at_ms, failed_since: summary.failed.is_some() })
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
            kept.summary.said_at_ms -= millis(by);
            kept.asked_at_ms -= millis(by);
            // What GitHub said keeps when it was read too, which is what a pull request's freshness goes by.
            match kept.summary.said.as_mut() {
                Some(Said::Found { reference }) => match &mut **reference {
                    Reference::Issue(issue) => issue.summary.read_at_ms -= millis(by),
                    Reference::PullRequest(pull_request) => pull_request.summary.read_at_ms -= millis(by),
                    Reference::Unknown => {}
                },
                Some(Said::Missing) | None => {}
            }
        }
        self.rules.age(by);
        self.pulls.age(by);
    }
}

/// `duration` in milliseconds, or the most there are.
pub(super) fn millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

/// `pull_request` with `checks`, as they were reported, judged by `rules`, as of when they were read.
fn judged(pull_request: PullRequest, checks: Read<Checks>, rules: Option<&Rules>) -> PullRequest {
    let said = github::judged(checks.said, pull_request.said(), rules);
    PullRequest { checks: Some(Read { said, read_at_ms: checks.read_at_ms }), ..pull_request }
}
