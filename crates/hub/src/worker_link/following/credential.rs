//! The hub's git credential, as a link follows it.

use protocol::client::{ApiError, ErrorCode};
use protocol::worker::{GitCredentialSet, GithubCredential};
use tokio::sync::watch;
use tracing::warn;

use super::super::WorkerLink;
use super::Followed;
use crate::github_logins::DefaultCredential;

/// What the hub has workers give their processes for github.com, or `None` for a worker on a provider whose machines
/// Roundhouse leaves GitHub to, as `Hub::leaves_github_to_machine` says, which is sent the machine's own as its link
/// opens, and nothing after.
pub(in super::super) struct Credential(pub(in super::super) Option<watch::Receiver<DefaultCredential>>);

impl Followed for Credential {
    type Set = GitCredentialSet;

    const CALLED: &'static str = "git credential";

    fn set(&mut self) -> GitCredentialSet {
        let credential = match &mut self.0 {
            Some(credential) => credential.borrow_and_update().for_workers(),
            None => GithubCredential::MachineOwn,
        };
        GitCredentialSet { credential }
    }

    fn applied(&mut self, _: &WorkerLink) {}

    async fn changed(&mut self) -> bool {
        match &mut self.0 {
            Some(credential) => credential.changed().await.is_ok(),
            None => false,
        }
    }

    /// One that answered that it can't apply it is used on, with the machine's own, as [`GitCredentialSet`] says, and
    /// the log says so.
    fn refused(&mut self, link: &WorkerLink, refused: ApiError) -> Result<(), ApiError> {
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
