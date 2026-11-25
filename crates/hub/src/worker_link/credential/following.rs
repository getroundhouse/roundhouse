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
use crate::github_logins::DefaultCredential;

/// How long a worker has to answer a set, from when it's queued. A worker applies one by writing three small files,
/// and the first time, asking its git's version, so it answers in milliseconds, and one that hasn't by now is taken for
/// stuck, as one that doesn't answer its hello is. Not something a test from outside would sit through, so not one of
/// [`crate::Timers`].
pub(crate) const SET_WITHIN: Duration = Duration::from_secs(10);

/// One link's following of what the hub has workers give their processes for github.com, which sends the worker each
/// credential the hub has, in turn, each once the worker has answered the one before. It sends the newest each time,
/// so one that changes twice while the worker applies a set is sent only the second after it. Each link follows on a
/// task of its own, so a slow worker holds up only its own link. A worker on a provider whose machines Roundhouse
/// leaves GitHub to is sent the machine's own as its link opens, and nothing after.
pub(crate) struct Following {
    link: Arc<WorkerLink>,
    /// What the hub has workers give their processes for github.com, or `None` for a worker on a provider whose
    /// machines Roundhouse leaves GitHub to, as `Hub::leaves_github_to_machine` says.
    credential: Option<watch::Receiver<DefaultCredential>>,
}

impl Following {
    pub(crate) fn new(link: Arc<WorkerLink>, credential: Option<watch::Receiver<DefaultCredential>>) -> Self {
        Self { link, credential }
    }

    /// Queues the set of the credential the hub has now, or of the machine's own, and returns the worker's answer to
    /// wait for, which [`Following::answered`] reads.
    pub(crate) async fn queue(&mut self) -> Result<impl Future<Output = Result<(), ApiError>> + use<>, ApiError> {
        let credential = match &mut self.credential {
            Some(credential) => credential.borrow_and_update().for_workers(),
            None => GithubCredential::MachineOwn,
        };
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
            let Some(credential) = self.credential.as_mut() else { return };
            tokio::select! {
                changed = credential.changed() => if changed.is_err() { return },
                () = self.link.wait_closed() => return,
            }
            // Queueing waits too, while the link's lines aren't being written.
            let answered = tokio::time::timeout(SET_WITHIN, async { self.queue().await?.await }).await;
            let Err(e) = self.taken(answered) else { continue };
            // One that's closing, as when the hub stops it or leaves it to the next hub, has had its last set.
            let link = &self.link;
            if link.is_open() {
                warn!(
                    name: "worker.credential_failed",
                    link = %link,
                    error = e.message,
                    "Ending a worker's link, which failed its git credential"
                );
                link.kill();
            }
            return;
        }
    }

    /// What an answer that came, or didn't, within [`SET_WITHIN`] comes to, as [`Following::answered`] says.
    fn taken(&self, answered: Result<Result<(), ApiError>, Elapsed>) -> Result<(), ApiError> {
        let refused = match answered {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(refused)) => refused,
            Err(_) => return Err(not_answered()),
        };
        let link = &self.link;
        let message = &refused.message;
        match refused.code {
            ErrorCode::Unavailable => {
                warn!(
                    name: "worker.credential_unapplied",
                    link = %link,
                    error = message,
                    "A worker couldn't apply its git credential, so what it starts uses its machine's own"
                );
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
            | ErrorCode::Unknown
            | ErrorCode::Unauthorized
            | ErrorCode::LocalOnly
            | ErrorCode::Unsupported => Err(refused),
        }
    }
}

fn not_answered() -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("the worker didn't answer its git credential within {SET_WITHIN:?}"))
}
