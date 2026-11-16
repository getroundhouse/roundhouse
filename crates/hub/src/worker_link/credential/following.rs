//! One link's following of the hub's git credential, and what each answer to a set comes to.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::worker::{GitCredentialSet, GithubCredential};
use tokio::sync::watch;
use tokio::time::error::Elapsed;
use tracing::warn;

use super::super::WorkerLink;

/// How long a worker has to answer a set, from when it's queued. A worker applies one by writing three small files,
/// and the first time, asking its git's version, so it answers in milliseconds, and one that hasn't by now is taken for
/// stuck, as one that doesn't answer its hello is. Not something a test from outside would sit through, so not one of
/// [`crate::Timers`].
pub(crate) const SET_WITHIN: Duration = Duration::from_secs(10);

/// One link's following of what the hub has workers give their processes for github.com, which sends the worker each
/// credential the hub has, in turn, each once the worker has answered the one before. It sends the newest each time,
/// so one that changes twice while the worker applies a set is sent only the second after it.
pub(crate) struct Following {
    link: Arc<WorkerLink>,
    credential: watch::Receiver<GithubCredential>,
    /// Whether the log has said the worker doesn't know the request, which it says once a link.
    said_unsupported: bool,
}

impl Following {
    pub(crate) fn new(link: Arc<WorkerLink>, credential: watch::Receiver<GithubCredential>) -> Self {
        Self { link, credential, said_unsupported: false }
    }

    /// Queues the set of the credential the hub has now, and returns the worker's answer to wait for, which
    /// [`Following::answered`] reads.
    pub(crate) async fn queue(&mut self) -> Result<impl Future<Output = Result<(), ApiError>> + use<>, ApiError> {
        let credential = self.credential.borrow_and_update().clone();
        self.link.send(GitCredentialSet { credential }).await
    }

    /// What the worker's answer to a set, `answer`, comes to for the link, once it has come within [`SET_WITHIN`]. A
    /// worker that applied it, or answered that it can't, is used on, with the machine's own in the second case, as
    /// [`GitCredentialSet`] says, and the log says which. One that doesn't answer in time, or whose connection ended
    /// first, or that answered anything else, has failed, and that's the error.
    pub(crate) async fn answered(
        &mut self,
        answer: impl Future<Output = Result<(), ApiError>>,
    ) -> Result<(), ApiError> {
        let answered = tokio::time::timeout(SET_WITHIN, answer).await;
        self.taken(answered)
    }

    /// Sends each credential the hub has after the last sent, in turn, until the link closes, or the hub goes. A link
    /// whose worker fails a set, as [`Following::answered`] says, is ended, as a worker that fails is, so it can't go
    /// on pushing as a default that's since been disconnected: its going tells its sessions, and the next worker
    /// started there is sent the credential as it opens.
    pub(crate) async fn follow(mut self) {
        loop {
            tokio::select! {
                changed = self.credential.changed() => if changed.is_err() { return },
                () = self.link.wait_closed() => return,
            }
            // Queueing waits too, while the link's lines aren't being written.
            let answered = tokio::time::timeout(SET_WITHIN, async { self.queue().await?.await }).await;
            let Err(e) = self.taken(answered) else { continue };
            // One that's closing, as when the hub stops it or leaves it to the next hub, has had its last set.
            let link = &self.link;
            if link.is_open() {
                warn!("ending {link}, which failed its git credential: {}", e.message);
                link.kill();
            }
            return;
        }
    }

    /// What an answer that came, or didn't, within [`SET_WITHIN`] comes to, as [`Following::answered`] says.
    fn taken(&mut self, answered: Result<Result<(), ApiError>, Elapsed>) -> Result<(), ApiError> {
        let refused = match answered {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(refused)) => refused,
            Err(_) => return Err(not_answered()),
        };
        let link = &self.link;
        let message = &refused.message;
        match refused.code {
            // From a worker from before sets, which is the same on every set, so it's said once.
            ErrorCode::Unsupported => {
                if !std::mem::replace(&mut self.said_unsupported, true) {
                    warn!("{link} doesn't take a git credential, so what it starts uses its machine's own: {message}");
                }
                Ok(())
            }
            ErrorCode::Unavailable => {
                warn!("{link} couldn't apply its git credential, so what it starts uses its machine's own: {message}");
                Ok(())
            }
            // Nothing a worker that applies sets answers, so nothing says what it has applied.
            ErrorCode::VersionMismatch
            | ErrorCode::NotFound
            | ErrorCode::InvalidRequest
            | ErrorCode::Conflict
            | ErrorCode::NeedsForce
            | ErrorCode::NameTaken
            | ErrorCode::Worker
            | ErrorCode::Internal
            | ErrorCode::Unknown => Err(refused),
        }
    }
}

fn not_answered() -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("the worker didn't answer its git credential within {SET_WITHIN:?}"))
}
