//! Telling a worker whether to watch as its workspace starts being archived and as the move ends.

use protocol::client::ErrorCode;
use protocol::model::{LineCounts, Observed};
use protocol::worker::WorkerRequest;

use super::stand_in::{Watch, by, frame, next_lines, summary, until_watches, w, watches, worker};
use crate::hub::Followed;
use crate::hub::tests::hub_with_worktree;

/// A followed workspace being archived has its worker told to stop watching before anything of its worktree is read
/// or removed. Archiving it is refused, since that would lose a change that isn't committed, which puts it back as
/// ready, and its worker is told to watch again, so what it sends next is current.
#[tokio::test]
async fn a_workspace_whose_archive_is_refused_is_watched_again() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (link, asked) = worker(&hub, Watch::Watching);
    let _following = hub.follow(by(1), [Followed::Workspace(w())]);
    until_watches(&asked, 1).await;

    let refused = hub.workspace_archive(w(), false).await;
    let mut feed = hub.subscribe_feed();
    hub.changes_frame(&w(), &link, frame(summary(5, 0))).await;
    let lines = next_lines(&mut feed).await;

    assert_eq!(refused.map_err(|e| e.code).unwrap_err(), ErrorCode::NeedsForce);
    let told: Vec<bool> = watches(&asked).iter().map(|asked| asked.watching).collect();
    assert_eq!(told, [true, false, true], "stopped, then watching again");
    let at = |asked_for: fn(&WorkerRequest) -> bool| asked.lock().iter().position(asked_for).unwrap();
    let stopped = at(|request| matches!(request, WorkerRequest::WatchChanges(asked) if !asked.watching));
    let read = at(|request| matches!(request, WorkerRequest::GitStatus(_)));
    assert!(stopped < read, "the status was read before the watch stopped");
    assert_eq!(lines, Observed::Current { value: LineCounts { added: 5, removed: 0 } });
}
