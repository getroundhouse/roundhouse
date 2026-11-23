//! What tells the task that works out GitHub's observations that a read of a workspace's pull request has ended,
//! whether it failed, and what it heard of the workspace's repository, which nothing the cache keeps says.

use parking_lot::Mutex;
use protocol::model::{RepositoryUnseen, WorkspaceId};
use tokio::sync::mpsc;

use crate::hub::Hub;
use crate::store::now_ms;

/// Tells of each read of a workspace's pull request as it ends, in a queue that loses none, since what a read heard of
/// the repository is kept nowhere else until the task keeps it.
pub(crate) struct ReadsEnded {
    queue: mpsc::UnboundedSender<ReadEnded>,
    /// The end of the queue the task reads, until it takes it as it starts.
    queued: Mutex<Option<mpsc::UnboundedReceiver<ReadEnded>>>,
}

impl Default for ReadsEnded {
    fn default() -> Self {
        let (queue, queued) = mpsc::unbounded_channel();
        Self { queue, queued: Mutex::new(Some(queued)) }
    }
}

/// A read of a workspace's pull request that has ended, once what it found is recorded.
#[derive(Debug)]
pub(super) struct ReadEnded {
    pub(super) workspace: WorkspaceId,
    /// Whether it couldn't be read, as while GitHub can't be reached, or GitHub showed no repository to read it in.
    pub(super) failed: bool,
    pub(super) heard: Heard,
    /// When it ended, by the hub's clock, which is when what it heard was heard.
    pub(super) at_ms: i64,
}

/// What a read of a workspace's pull request heard of whether GitHub showed the default the repository where the
/// branch's pull requests are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Heard {
    /// GitHub showed no repository there, as the default, as this says.
    Unseen(RepositoryUnseen),
    /// GitHub showed the repository, as a read that found the pull request, or that there's none, shows.
    Seen,
    /// Nothing, as from a read that failed otherwise, or from GitHub showing none while there's no default to say it of.
    Nothing,
}

impl ReadsEnded {
    /// The end of the queue the task reads, which only the first to ask is given, since each read's end is told once.
    pub(super) fn take(&self) -> Option<mpsc::UnboundedReceiver<ReadEnded>> {
        self.queued.lock().take()
    }

    /// Puts back the end of the queue a test took, for it to take again.
    #[cfg(test)]
    pub(super) fn give_back(&self, queued: mpsc::UnboundedReceiver<ReadEnded>) {
        *self.queued.lock() = Some(queued);
    }
}

impl Hub {
    /// Tells of a read of `workspace`'s pull request that has ended, once what it found is recorded, failed or not,
    /// with what it heard of the repository.
    pub(crate) fn github_read_ended(&self, workspace: &WorkspaceId, failed: bool, heard: Heard) {
        let ended = ReadEnded { workspace: workspace.clone(), failed, heard, at_ms: now_ms() };
        // Fails only once the task's end is dropped, as the hub stops, which leaves nobody to tell.
        let _ = self.github_reads_ended.queue.send(ended);
    }
}
