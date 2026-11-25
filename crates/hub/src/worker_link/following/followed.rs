//! One link's following of something the hub has its worker use, whatever it is, and what each answer to a set comes to.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::worker::Request;
use tokio::time::error::Elapsed;
use tracing::warn;

use super::super::WorkerLink;

/// How long a worker has to answer a set, from when it's queued. A worker applies one in memory, or by writing three
/// small files, and the first time, asking its git's version, so it answers in milliseconds, and one that hasn't by now
/// is taken for stuck, as one that doesn't answer its hello is. Not something a test from outside would sit through, so
/// not one of [`crate::Timers`].
pub(super) const SET_WITHIN: Duration = Duration::from_secs(10);

/// Something the hub has a worker use, as it changes, whose newest a link sets whole on its worker.
pub(in super::super) trait Followed: Send + 'static {
    type Set: Request<Reply = ()>;

    /// What the log calls it, as a worker's, such as `git credential`.
    const CALLED: &'static str;

    /// Sets what the hub has now, which counts as the last sent from then on.
    fn set(&mut self) -> Self::Set;

    /// Once the hub has another than the last sent, which says `true`, or `false` once the link has nothing to follow.
    fn changed(&mut self) -> impl Future<Output = bool> + Send;

    /// What the worker's `refused` comes to for `link`: the worker is used on, or has failed, and that's the error.
    fn refused(&mut self, link: &WorkerLink, refused: ApiError) -> Result<(), ApiError>;
}

/// One link's following of what it sets on its worker, which sends the worker each one the hub has, in turn, each once
/// the worker has answered the one before. It sends the newest each time, so one that changes twice while the worker
/// applies a set is sent only the second after it. Each link follows on a task of its own, so a slow worker holds up
/// only its own link.
pub(in super::super) struct Following<F> {
    link: Arc<WorkerLink>,
    followed: F,
}

impl<F: Followed> Following<F> {
    pub(in super::super) fn new(link: Arc<WorkerLink>, followed: F) -> Self {
        Self { link, followed }
    }

    /// Queues the set of what the hub has now, and returns the worker's answer to wait for, which
    /// [`Following::answered`] reads.
    pub(in super::super) async fn queue(
        &mut self,
    ) -> Result<impl Future<Output = Result<(), ApiError>> + use<F>, ApiError> {
        let set = self.followed.set();
        self.link.send(set).await
    }

    /// What the worker's answer to a set, `answer`, comes to for the link, once it has come within [`SET_WITHIN`]. A
    /// worker that applied it is used on, and one that refused it as [`Followed::refused`] says. One that doesn't
    /// answer in time, or whose connection ended first, has failed, and that's the error.
    pub(in super::super) async fn answered(
        &mut self,
        answer: impl Future<Output = Result<(), ApiError>>,
    ) -> Result<(), ApiError> {
        let answered = tokio::time::timeout(SET_WITHIN, answer).await;
        self.taken(answered)
    }

    /// Sends each one the hub has after the last sent, in turn, until the link closes, or there's nothing to follow. A
    /// link whose worker fails a set, as [`Following::answered`] says, is ended, as a worker that fails is, so it can't
    /// go on with what the hub no longer has, such as pushing as a default that's since been disconnected: its going
    /// tells its sessions, and the next worker started there is sent the newest as it opens.
    pub(in super::super) async fn follow(mut self) {
        loop {
            tokio::select! {
                changed = self.followed.changed() => if !changed { return },
                () = self.link.wait_closed() => return,
            }
            // Queueing waits too, while the link's lines aren't being written.
            let answered = tokio::time::timeout(SET_WITHIN, async { self.queue().await?.await }).await;
            let Err(e) = self.taken(answered) else { continue };
            // One that's closing, as when the hub stops it or leaves it to the next hub, has had its last set.
            let link = &self.link;
            if link.is_open() {
                warn!(
                    name: "worker.set_failed",
                    link = %link,
                    what = F::CALLED,
                    error = e.message,
                    "Ending a worker's link, which failed what the hub set on it"
                );
                link.kill();
            }
            return;
        }
    }

    /// What an answer that came, or didn't, within [`SET_WITHIN`] comes to, as [`Following::answered`] says.
    fn taken(&mut self, answered: Result<Result<(), ApiError>, Elapsed>) -> Result<(), ApiError> {
        match answered {
            Ok(Ok(())) => Ok(()),
            Ok(Err(refused)) => self.followed.refused(&self.link, refused),
            Err(_) => {
                let message = format!("the worker didn't answer its {} within {SET_WITHIN:?}", F::CALLED);
                Err(ApiError::new(ErrorCode::Worker, message))
            }
        }
    }
}
