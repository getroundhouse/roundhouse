//! What each open wait last read, which the hub keeps as the observation `WaitRead`, apart from the wait's record, and
//! judges the wait from, worked out from GitHub's part of the cache as each read of its pull requests ends. As a wait
//! ends, its record takes what it last read, frozen, and the observation goes.

use std::collections::HashMap;

use protocol::client::ApiError;
use protocol::model::{IssueNumber, PullRequestsSnapshot, Wait, WaitId, WorkspaceId};
use tracing::warn;

use super::{Arc, Hub};
use crate::github_cache::Item;
use crate::hub::observations::{WaitKey, WaitRead};
use crate::store::Store;

impl Hub {
    /// Judges each of the workspace's open waits as `revise` says, from what it last read, and keeps what each read
    /// now, or, for one that ended, forgets it, since its record keeps it frozen. Under the lock every judging and
    /// settling of waits takes, so what's kept is worked out from the newest of what each read, and a wait that ends
    /// meanwhile is settled after. Returns those that changed, with what they read.
    pub(super) async fn judge_waits(
        self: &Arc<Self>,
        id: &WorkspaceId,
        revise: impl FnMut(&mut Wait) + Send + 'static,
    ) -> Vec<Wait> {
        let _judging = self.waits_judged.lock().await;
        let (id, reads) = (id.clone(), self.wait_reads());
        let changed = self.blocking(move |store| store.revise_remote_waits(Some(&id), &reads, revise)).await;
        let waits = match changed {
            Ok(waits) => waits,
            Err(e) => {
                warn!("judging waits on what was read of their pull requests: {}", e.message);
                return Vec::new();
            }
        };
        for wait in &waits {
            self.keep_wait_read(wait).await;
        }
        waits
    }

    /// Records `wait`, which has just started from a read of its pull requests, and keeps what it read, unless its call
    /// started one before, which it returns instead. Either with what it read.
    pub(crate) async fn start_wait(&self, wait: Wait) -> Result<Wait, ApiError> {
        let asked = wait.clone();
        let recorded = self.blocking(move |store| store.start_wait(&asked)).await?;
        if recorded.id == wait.id {
            self.keep_wait_read(&wait).await;
        }
        Ok(self.as_read(recorded))
    }

    /// Keeps what `wait`, which a judging just changed, read, or forgets it once the wait has ended.
    async fn keep_wait_read(&self, wait: &Wait) {
        let Some(watch) = &wait.watch else { return };
        let of = key_of(wait);
        let kept = if wait.ended.is_some() {
            self.observation_gone::<WaitRead>(of).await
        } else {
            let read = watch.read();
            let as_of_ms = read.observed_at_ms;
            self.observation_worked_out::<WaitRead>(of, read, as_of_ms).await
        };
        if let Err(e) = kept {
            warn!(wait = %wait.id, "keeping what a wait read: {}", e.message);
        }
    }

    /// What each open wait last read, by wait, as the hub keeps it.
    fn wait_reads(&self) -> HashMap<WaitId, protocol::model::WaitRead> {
        let kept = self.observations.kept_of::<WaitRead>().into_iter();
        let reads = kept.filter_map(|of| Some((of.wait.clone(), self.observations.value_of::<WaitRead>(&of)?)));
        reads.collect()
    }

    /// `wait`, with what it last read in its watch, where it's open and the hub keeps that, for what the hub decides of
    /// it and what it tells the agent.
    pub(crate) fn as_read(&self, mut wait: Wait) -> Wait {
        self.fill_old_forms(&mut wait);
        wait
    }

    /// Keeps in its record what each wait that has ended last read, where the hub kept it apart, and forgets it, as
    /// after something other than a judging ended it: the agent, the person, a deadline, closing its chat or archiving
    /// its workspace. Under the lock a judging takes, so what one kept of a wait that ended meanwhile is settled too.
    pub(crate) async fn wait_reads_settled(&self) {
        let _judging = self.waits_judged.lock().await;
        let open = match self.blocking(Store::open_wait_ids).await {
            Ok(open) => open,
            Err(e) => return warn!("listing the open waits to settle what ended ones read: {}", e.message),
        };
        for of in self.observations.kept_of::<WaitRead>() {
            if open.contains(&of.wait) {
                continue;
            }
            if let Some(read) = self.observations.value_of::<WaitRead>(&of) {
                let id = of.wait.clone();
                if let Err(e) = self.blocking(move |store| store.freeze_wait_read(&id, &read)).await {
                    warn!(wait = %of.wait, "keeping what an ended wait last read: {}", e.message);
                }
            }
            if let Err(e) = self.observation_gone::<WaitRead>(of).await {
                warn!("forgetting what an ended wait read: {}", e.message);
            }
        }
    }

    /// `read`, a read of pull requests just made, as GitHub's part of the cache keeps what it read, which is what a
    /// wait's read is worked out from. One the cache doesn't keep is left out, as it would be from a read that
    /// didn't find it.
    pub(super) fn as_kept(&self, read: &PullRequestsSnapshot) -> PullRequestsSnapshot {
        let kept = read.pull_requests.iter().filter_map(|pull_request| {
            let number = IssueNumber::try_from(pull_request.number.get()).ok()?;
            self.github_cache.pull_request(&Item::new(&pull_request.repository, number))
        });
        PullRequestsSnapshot { pull_requests: kept.collect(), ..read.clone() }
    }
}

/// What the hub keeps what `wait` read by.
fn key_of(wait: &Wait) -> WaitKey {
    WaitKey { workspace_id: wait.workspace_id.clone(), session_id: wait.session_id.clone(), wait: wait.id.clone() }
}
