//! One thing at a time for each worktree: making another of the workspace's, removing one, or putting the
//! workspace's own back. Each goes on when the hub that asked has gone, so the next hub's question, or its asking
//! again, waits for it and finds how it ended. Each worktree apart, so a slow add of one holds up no other.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;

/// The worktrees being made, removed or put back, by path, as the hub spells it, which is the one spelling it has.
/// A second add of one, as from a new hub asking again what the last didn't hear back about, waits for the first and
/// then finds it made, a removal waits for an add, and whether one is there is answered once nothing is changing
/// that.
#[derive(Default)]
pub(super) struct Moves(Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>);

impl Moves {
    /// Once nothing else is making, removing or putting back the worktree at `path`, until what it returns is dropped.
    pub(super) async fn alone(&self, path: &Path) -> Alone<'_> {
        let lock = self.0.lock().entry(path.to_path_buf()).or_default().clone();
        Alone { moves: self, path: path.to_path_buf(), held: lock.lock_owned().await }
    }
}

/// The only thing moving a worktree, until it's dropped.
pub(super) struct Alone<'a> {
    moves: &'a Moves,
    path: PathBuf,
    held: tokio::sync::OwnedMutexGuard<()>,
}

impl Drop for Alone<'_> {
    fn drop(&mut self) {
        let mut moves = self.moves.0.lock();
        // Held only by the map and this, so nothing else waits for it.
        if Arc::strong_count(tokio::sync::OwnedMutexGuard::mutex(&self.held)) == 2 {
            moves.remove(&self.path);
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::time::Duration;

    use super::*;

    /// Something that mustn't happen has no condition to wait on, so this is a bound. Right code never gets past it.
    pub(crate) const BOUND: Duration = Duration::from_millis(300);

    /// So a slow add of one worktree holds up no other, nor the workspace's own.
    #[tokio::test]
    async fn one_worktree_s_turn_holds_up_no_other() {
        let moves = Moves::default();

        let _held = moves.alone(Path::new("/c/a")).await;
        let other = tokio::time::timeout(BOUND, moves.alone(Path::new("/c/b"))).await;

        assert!(other.is_ok(), "another worktree waited");
    }

    /// Each takes its turn in the order it came, and the last to leave lets go of the path, so nothing is kept for a
    /// worktree nobody is moving.
    #[tokio::test]
    async fn each_waits_for_the_one_before_it_on_the_same_worktree() {
        let moves = Moves::default();
        let path = Path::new("/c/a");
        let first = moves.alone(path).await;
        let second = moves.alone(path);
        tokio::pin!(second);
        let early = tokio::time::timeout(BOUND, &mut second).await;
        assert!(early.is_err(), "the second didn't wait for the first");

        drop(first);
        let second = second.await;
        let third = moves.alone(path);
        tokio::pin!(third);
        let beside_the_second = tokio::time::timeout(BOUND, &mut third).await;
        assert!(beside_the_second.is_err(), "the third didn't wait for the second");
        drop(second);
        drop(third.await);

        assert!(moves.0.lock().is_empty(), "the path is let go of once nobody has it");
    }
}
