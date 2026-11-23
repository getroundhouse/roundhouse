//! What tells the task that works out GitHub's observations that a read of a workspace's pull request has ended, and
//! whether it failed, which nothing the cache keeps says.

use protocol::model::WorkspaceId;
use tokio::sync::broadcast;

use crate::hub::Hub;

/// How many ends of reads the task can fall behind by before it works out everything again.
const TOLD_BEHIND: usize = 64;

/// Tells of each read of a workspace's pull request as it ends.
pub(crate) struct ReadsEnded(broadcast::Sender<ReadEnded>);

impl Default for ReadsEnded {
    fn default() -> Self {
        Self(broadcast::Sender::new(TOLD_BEHIND))
    }
}

/// A read of a workspace's pull request that has ended, once what it found is kept.
#[derive(Clone, Debug)]
pub(super) struct ReadEnded {
    pub(super) workspace: WorkspaceId,
    /// Whether it couldn't be read, as while GitHub can't be reached, or GitHub showed no repository to read it in.
    pub(super) failed: bool,
}

impl ReadsEnded {
    pub(super) fn subscribe(&self) -> broadcast::Receiver<ReadEnded> {
        self.0.subscribe()
    }
}

impl Hub {
    /// Tells of a read of `workspace`'s pull request that has ended, once what it found is kept, failed or not. Nobody
    /// may be listening.
    pub(crate) fn github_read_ended(&self, workspace: &WorkspaceId, failed: bool) {
        let _ = self.github_reads_ended.0.send(ReadEnded { workspace: workspace.clone(), failed });
    }
}
