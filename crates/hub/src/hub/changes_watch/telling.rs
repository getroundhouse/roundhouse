//! Telling a workspace's worker whether to watch its worktree, as the hub's list of followers says, and taking in what
//! it sends.

use std::sync::Arc;

use protocol::client::ErrorCode;
use protocol::model::WorkspaceId;
use protocol::worker::{WatchChanges, WorkerFrame};
use tracing::{debug, warn};

use super::watches::{Answer, Told, Turn};
use crate::hub::observations::{WorkspaceChanges, WorkspaceLines};
use crate::hub::{Followed, Hub};
use crate::store::now_ms;
use crate::worker_link::WorkerLink;

impl Hub {
    /// Tells the workspace's worker whether to watch its worktree: it watches while a subscription follows the
    /// workspace and the workspace is ready, and only a worker running already is told, as the module says. Each change
    /// to whether it watches is kept, so its lines and changed files are current while it does, and as of when it stopped
    /// once it does. The worker answers at once, and sends what it sees after, so it's given only so long, since a call
    /// left waiting on it would hold the workspace's turn to be told, and an archive of it, and keep the workspace from
    /// being idle, so its worker from being replaced.
    pub(crate) async fn changes_watching(self: &Arc<Self>, id: &WorkspaceId) {
        let mut told = self.changes_watches.turn(id).await;
        // Read in its turn, so what's told last is what's so.
        let followed = self.followers.follows(&Followed::Workspace(id.clone()));
        let ready = if followed { self.ready_workspace(id).await.ok() } else { None };
        let link = self.running_worker(id);
        let (Some(ready), Some(link)) = (ready, link) else {
            self.stop_watching(id, &mut told).await;
            return;
        };
        let base_ref = ready.workspace().base_ref.clone();
        if let Some(was) = told.as_ref().filter(|was| was.link == link.id) {
            match was.answer {
                Answer::Refused => return,
                Answer::Watching if was.base_ref == base_ref => return,
                Answer::Watching | Answer::Late => {}
            }
        }
        // Before it's asked, so its first summary, which may come before the reply, is current.
        self.changes_kept_current(id).await;
        let asked = WatchChanges { path: ready.path().to_owned(), base_ref: base_ref.clone(), watching: true };
        match link.call_at_once(asked).await {
            Ok(Ok(())) => *told = Some(Told { link: link.id, base_ref, answer: Answer::Watching }),
            // A worker from before the request, which the hub replaces once nothing in its workspace is at work, and
            // whose summary is read once now instead, and as each turn ends.
            Ok(Err(e)) if e.code == ErrorCode::Unsupported => {
                debug!(%id, "the worker can't watch its worktree: {}", e.message);
                *told = Some(Told { link: link.id, base_ref, answer: Answer::Refused });
                self.changes_let_go(id).await;
                self.read_lines(id, &link);
            }
            Ok(Err(e)) => {
                warn!(%id, "asking the worker to watch its worktree: {}", e.message);
                *told = None;
                self.changes_let_go(id).await;
            }
            Err(late) => {
                warn!(%id, "asking the worker to watch its worktree: {late}");
                *told = Some(Told { link: link.id, base_ref, answer: Answer::Late });
                self.changes_let_go(id).await;
            }
        }
    }

    /// Tells the workspace's worker, as [`Hub::changes_watching`] does, on a task of its own, for where nothing waits
    /// for it, such as a subscription that has gone.
    pub(crate) fn changes_watching_soon(self: &Arc<Self>, id: &WorkspaceId) {
        let (hub, id) = (self.clone(), id.clone());
        // Spawning panics outside a runtime, and nothing stops a following being dropped outside one.
        let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
        runtime.spawn(async move { hub.changes_watching(&id).await });
    }

    /// Takes in that `link`, the workspace's worker, has gone, so nothing watches its worktree.
    pub(crate) async fn changes_watcher_gone(&self, id: &WorkspaceId, link: &WorkerLink) {
        let mut told = self.changes_watches.turn(id).await;
        if told.as_ref().is_some_and(|was| was.link == link.id) {
            *told = None;
            self.changes_let_go(id).await;
        }
    }

    /// The summary in a frame from the workspace's worker, `link`, if it's one, which is kept only if `link` is the
    /// workspace's worker now and it's of the workspace's own worktree. Gives back a frame that isn't one.
    pub(crate) async fn changes_frame(
        self: &Arc<Self>,
        id: &WorkspaceId,
        link: &Arc<WorkerLink>,
        frame: WorkerFrame,
    ) -> Option<WorkerFrame> {
        let WorkerFrame::Changes { path, summary } = frame else { return Some(frame) };
        let read_at_ms = now_ms();
        if !link.is_current(&self.workers.lock(), id) {
            return None;
        }
        if self.ready_workspace(id).await.is_ok_and(|ready| ready.path() == path) {
            self.changes_read(id, summary, read_at_ms).await;
        }
        None
    }

    /// Tells the worker that was told to watch to stop, if it's still the workspace's, and keeps that nothing watches,
    /// whether it answered that it watches or didn't answer in time. A worker that refused is left as it was, so it
    /// isn't asked again once the workspace is followed again.
    async fn stop_watching(&self, id: &WorkspaceId, told: &mut Turn<'_>) {
        let Some(was) = told.take_if(|was| was.answer != Answer::Refused) else { return };
        self.changes_let_go(id).await;
        let Some(link) = self.running_worker(id).filter(|link| link.id == was.link) else { return };
        let Ok(ready) = self.workspace(id).await.map(|workspace| workspace.path) else { return };
        let Some(path) = ready else { return };
        let stop = WatchChanges { path, base_ref: was.base_ref, watching: false };
        match link.call_at_once(stop).await.unwrap_or_else(|late| Err(late.into())) {
            Ok(()) => {}
            Err(e) if e.code == ErrorCode::NotFound => {}
            Err(e) => debug!(%id, "telling the worker to stop watching its worktree: {}", e.message),
        }
    }

    /// Keeps that the worker watching the workspace's worktree keeps both observations of its changes up to date from
    /// now on: its lines, and its changed files.
    async fn changes_kept_current(&self, id: &WorkspaceId) {
        if let Err(e) = self.observation_kept_current::<WorkspaceLines>(id.clone()).await {
            warn!(%id, "keeping that the workspace's lines are watched: {}", e.message);
        }
        if let Err(e) = self.observation_kept_current::<WorkspaceChanges>(id.clone()).await {
            warn!(%id, "keeping that the workspace's changed files are watched: {}", e.message);
        }
    }

    /// Keeps that nothing keeps either observation of the workspace's changes up to date any more, as of now, since
    /// what watched them would have said of any change until then. That's the owner's choice. The other was as of the
    /// last summary that differed, which can show an hour-old time for one that was right a moment ago.
    async fn changes_let_go(&self, id: &WorkspaceId) {
        let as_of_ms = now_ms();
        if let Err(e) = self.observation_let_go::<WorkspaceLines>(id.clone(), as_of_ms).await {
            warn!(%id, "keeping that the workspace's lines are no longer watched: {}", e.message);
        }
        if let Err(e) = self.observation_let_go::<WorkspaceChanges>(id.clone(), as_of_ms).await {
            warn!(%id, "keeping that the workspace's changed files are no longer watched: {}", e.message);
        }
    }
}
