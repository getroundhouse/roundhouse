//! Stopping the hub, and what becomes of its workers and their agents as it stops: they stop with it, or carry on for
//! the next hub.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tracing::warn;

use super::{Hub, KEEPS_WORKERS, SHUTDOWN_GRACE};
use crate::session::Item;
use crate::store::Store;

impl Hub {
    pub(crate) fn request_shutdown(&self) {
        self.shutdown.notify_one();
    }

    /// Stops the hub and its workers, with their agents, even once it was asked to restart, since a stop says nothing
    /// carries on with them.
    pub(crate) fn request_stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        self.request_shutdown();
    }

    /// Stops once no setup script is running, and no request but this is under way, leaving the workers running for
    /// the next hub, so their agents carry on.
    pub(crate) fn request_restart(self: &Arc<Self>) -> Result<(), ApiError> {
        if !KEEPS_WORKERS {
            return Err(ApiError::new(ErrorCode::Unsupported, "a restart would stop this hub's agents, so it stops"));
        }
        if self.restarting.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let hub = self.clone();
        tokio::spawn(async move {
            // Counted as it's read, so this request is among them until its reply has gone.
            while !hub.setup_gates.lock().is_empty() || hub.requests.load(Ordering::SeqCst) > 1 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            hub.request_shutdown();
        });
        Ok(())
    }

    /// Whether the hub is going to restart, which leaves its workers running: it was asked to, and not to stop since.
    pub(crate) fn restarting(&self) -> bool {
        self.restarting.load(Ordering::SeqCst) && !self.stopping.load(Ordering::SeqCst)
    }

    /// Stops if nothing would notice, as the one asking sees it, and refuses otherwise. What that client sent before
    /// this either still counts as a request or has left its mark, such as an agent at work, since a prompt's agent
    /// is at work from when the prompt is recorded.
    pub(crate) fn shutdown_if_idle(&self) -> Result<(), ApiError> {
        if !self.status().is_idle() {
            return Err(ApiError::new(ErrorCode::Conflict, "the hub is busy, so it wasn't stopped"));
        }
        self.request_stop();
        Ok(())
    }

    /// Stops every worker and waits until what they said is recorded.
    pub(crate) async fn stop_all_workers(self: &Arc<Self>) {
        // Refuse new workers first, so a client still connected can't start one behind this.
        self.shutting_down.store(true, Ordering::SeqCst);
        // Before their agents go, so the exit of one whose only work is a wait the next hub keeps cuts nothing off.
        let waiting = self.blocking(Store::sessions_waiting).await.unwrap_or_default();
        let sessions: Vec<_> =
            self.sessions.lock().live.values().filter(|session| waiting.contains(&session.id)).cloned().collect();
        for session in sessions {
            session.outlive_this_hub().await;
        }
        let links: Vec<_> = self.workers.lock().iter().map(|(id, link)| (id.clone(), link.clone())).collect();
        for (_, link) in &links {
            link.close().await;
        }
        // One deadline for all of them, so several stuck workers don't add up.
        let deadline = tokio::time::Instant::now() + SHUTDOWN_GRACE;
        let mut stuck = JoinSet::new();
        for (workspace_id, link) in links {
            if tokio::time::timeout_at(deadline, link.wait_closed()).await.is_err() {
                warn!(%workspace_id, "a worker didn't stop in time");
                // The hub's exit would end the worker, but not its agents, which would run on unwatched.
                let hub = self.clone();
                stuck.spawn(async move { hub.end_stuck_worker_of(&workspace_id, &link).await });
            }
        }
        stuck.join_all().await;
        self.flush_sessions().await;
    }

    /// Leaves every worker running for the next hub, once it has heard what this hub recorded, and waits until what
    /// they sent is recorded, and each has let this hub go, so the next hub, which may start at once, isn't refused as
    /// a second hub.
    pub(crate) async fn leave_workers(self: &Arc<Self>) {
        self.shutting_down.store(true, Ordering::SeqCst);
        let links: Vec<_> = self.workers.lock().values().cloned().collect();
        for link in &links {
            link.leave(self).await;
        }
        let deadline = tokio::time::Instant::now() + SHUTDOWN_GRACE;
        for link in &links {
            if tokio::time::timeout_at(deadline, link.wait_closed()).await.is_err() {
                warn!("a worker didn't let this hub go in time");
                break;
            }
        }
        self.flush_sessions().await;
    }

    async fn flush_sessions(&self) {
        let sessions: Vec<_> = self.sessions.lock().live.values().cloned().collect();
        for session in sessions {
            let (done, flushed) = oneshot::channel();
            session.send_item(Item::Flush(done)).await;
            let _ = flushed.await;
        }
    }
}
