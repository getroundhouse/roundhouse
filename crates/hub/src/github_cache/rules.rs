use std::collections::HashMap;
use std::time::Duration;

use github::Rules;
use parking_lot::Mutex;
use protocol::model::{BranchName, GithubRepositoryName};
use tracing::warn;

use super::file::Row;
use super::kept::{KEPT_FOR, millis};
use super::keys::{BranchOf, Facet, Subject};

/// What the cache keeps of each branch's rules for the checks a pull request into it needs. A read of them that fails
/// fails the read of the pull requests into it, so nothing's kept of why.
#[derive(Default)]
pub(super) struct KeptRules(Mutex<HashMap<BranchOf, KeptRule>>);

struct KeptRule {
    rules: Rules,
    read_at_ms: i64,
    asked_at_ms: i64,
}

impl KeptRules {
    /// Takes in `row`, as the file kept it, if it's a branch's rules.
    pub(super) fn load(&self, row: &Row) {
        let (Subject::Branch(branch), Facet::Rules) = (&row.subject, row.facet) else { return };
        let Ok(rules) = serde_json::from_str(&row.said) else { return };
        let kept = KeptRule { rules, read_at_ms: row.read_at_ms, asked_at_ms: row.read_at_ms };
        self.0.lock().insert(branch.clone(), kept);
    }

    /// The rules of the branches of `repository` read within `fresh_for` by `now_ms`, which counts as their being
    /// asked about.
    pub(super) fn known(
        &self,
        repository: &GithubRepositoryName,
        fresh_for: Duration,
        now_ms: i64,
    ) -> Vec<(BranchName, Rules)> {
        let repository = BranchOf::new(repository, BranchName::from("")).repository().to_owned();
        let mut kept = self.0.lock();
        let fresh = kept
            .iter_mut()
            .filter(|(branch, kept)| branch.repository() == repository && now_ms - kept.read_at_ms < millis(fresh_for));
        fresh
            .map(|(branch, kept)| {
                kept.asked_at_ms = now_ms;
                (branch.branch().clone(), kept.rules.clone())
            })
            .collect()
    }

    /// Keeps `read`, each branch's rules as read at `now_ms`, and returns the rows the file keeps them as.
    pub(super) fn keep(&self, read: Vec<(BranchOf, Rules)>, now_ms: i64) -> Vec<Row> {
        let mut kept = self.0.lock();
        let mut rows = Vec::new();
        for (branch, rules) in read {
            match serde_json::to_string(&rules) {
                Ok(said) => rows.push(Row {
                    subject: Subject::Branch(branch.clone()),
                    facet: Facet::Rules,
                    said,
                    read_at_ms: now_ms,
                }),
                Err(error) => warn!("writing a branch's rules for the cache's file: {error}"),
            }
            kept.insert(branch, KeptRule { rules, read_at_ms: now_ms, asked_at_ms: now_ms });
        }
        rows
    }

    /// Forgets what nothing has asked for in [`KEPT_FOR`] by `now_ms`, and returns it.
    pub(super) fn forget_unasked(&self, now_ms: i64) -> Vec<Subject> {
        let mut forgotten = Vec::new();
        self.0.lock().retain(|branch, kept| {
            let keep = now_ms - kept.asked_at_ms < millis(KEPT_FOR);
            if !keep {
                forgotten.push(Subject::Branch(branch.clone()));
            }
            keep
        });
        forgotten
    }

    /// Makes what's kept as old as if `by` had gone by, for a test.
    #[cfg(test)]
    pub(super) fn age(&self, by: Duration) {
        for kept in self.0.lock().values_mut() {
            kept.read_at_ms -= millis(by);
            kept.asked_at_ms -= millis(by);
        }
    }
}
