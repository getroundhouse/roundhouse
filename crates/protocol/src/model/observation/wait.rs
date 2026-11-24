//! What the hub observes of an agent's wait on pull requests: what it last read of them.

use serde::{Deserialize, Serialize};

use crate::model::{PullRequest, PullRequestNumber, PullRequestWatch, PullRequestsSnapshot};

/// What an open wait last read of the pull requests it watches, worked out from GitHub's part of the hub's cache as
/// each read of them ends, and how the reads since went. What the wait asked for, what it saw as it started, the
/// repository it's pinned to and its deadline are its record's, as [`PullRequestWatch::recorded`] keeps them, and an
/// ended wait's record keeps its last read too, frozen as it ended.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WaitRead {
    /// Each pull request in its scope, with every facet, all read at `observed_at_ms`, as
    /// [`PullRequestsSnapshot::pull_requests`] has them.
    pub pull_requests: Vec<PullRequest>,
    /// When the read that found them began, by the hub's clock.
    pub observed_at_ms: i64,
    /// Why the last read after that one failed, if it did, as [`PullRequestWatch::read_error`] says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_error: Option<String>,
    /// When it was last read, or tried.
    pub last_attempt_at_ms: i64,
    /// The pull requests that ended it, or that the failed read was of, as [`PullRequestWatch::affected`] says.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected: Vec<PullRequestNumber>,
}

impl PullRequestWatch {
    /// What it last read, apart from what its record keeps.
    pub fn read(&self) -> WaitRead {
        WaitRead {
            pull_requests: self.snapshot.pull_requests.clone(),
            observed_at_ms: self.snapshot.observed_at_ms,
            read_error: self.read_error.clone(),
            last_attempt_at_ms: self.last_attempt_at_ms,
            affected: self.affected.clone(),
        }
    }

    /// What an open wait's record keeps of it: all but what it read since it started, which [`WaitRead`] says. Its
    /// snapshot keeps the repository and the time of the read it started from, with none of the pull requests.
    #[must_use]
    pub fn recorded(&self) -> Self {
        let snapshot = PullRequestsSnapshot { pull_requests: Vec::new(), ..self.snapshot.clone() };
        Self { snapshot, read_error: None, affected: Vec::new(), ..self.clone() }
    }

    /// It, with `read` as what it last read, in place of what its record has.
    #[must_use]
    pub fn with_read(self, read: WaitRead) -> Self {
        let WaitRead { pull_requests, observed_at_ms, read_error, last_attempt_at_ms, affected } = read;
        let snapshot = PullRequestsSnapshot { observed_at_ms, pull_requests, ..self.snapshot };
        Self { snapshot, read_error, last_attempt_at_ms, affected, ..self }
    }
}
