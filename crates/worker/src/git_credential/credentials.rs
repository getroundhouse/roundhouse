//! The git credential the hub set on this worker, and what every process the worker starts is given for it.

use std::path::Path;
use std::sync::Arc;

use process::Environment;
use protocol::client::{ApiError, ErrorCode};
use protocol::worker::GithubCredential;
use tokio::sync::{OnceCell, watch};
use tracing::warn;

use super::files::{Files, Handed};
use super::git_version::reads_the_config;
use super::helper::CredentialHelper;

/// Why agents push as nobody under a credential of a kind this worker doesn't know, which only a newer hub sends.
const NEWER_HUB: &str = "This workspace's worker is older than Roundhouse's hub, and can't use the GitHub account it was \
                         given, so its agents push as nobody until the hub replaces the worker with its own build.";

pub(crate) struct GitCredentials {
    files: Arc<Files>,
    /// What a process is given under the machine's own login: the machine's environment, with none of git's variables
    /// of a repository.
    machine: Arc<Environment>,
    /// What a process started now is given: [`Self::machine`] under the machine's own login, which is what a worker
    /// starts with, and that with the files otherwise.
    given: watch::Sender<Arc<Environment>>,
    /// Whether this machine's git reads the files, asked once, the first time it matters.
    git: OnceCell<Result<(), String>>,
}

impl GitCredentials {
    /// For a worker on a machine whose environment is `machine`, as [`crate::Config::machine`] says.
    pub(crate) fn new(state_dir: &Path, helper: &CredentialHelper, machine: Environment) -> Self {
        let files = Arc::new(Files::new(state_dir, helper.words()));
        let machine = Arc::new(crate::git::in_no_repository(machine));
        Self { files, given: watch::Sender::new(machine.clone()), machine, git: OnceCell::new() }
    }

    /// What a process is given under the machine's own login, whatever credential the hub has set, for what reads
    /// the machine's own, such as who its `gh` is logged in as.
    pub(crate) fn machine(&self) -> &Environment {
        &self.machine
    }

    /// What a process the worker starts now is given, for as long as it runs.
    pub(crate) fn given(&self) -> Arc<Environment> {
        self.given.borrow().clone()
    }

    /// What a process started at any time from now on is given, for something that starts processes for a while,
    /// such as a watch.
    pub(crate) fn watching(&self) -> watch::Receiver<Arc<Environment>> {
        self.given.subscribe()
    }

    /// Applies `credential`, so every process started once this returns has it, and every one started under one
    /// before reads it from its next `git` or `gh`. Refused as `Unavailable` where it can't be applied, as with a git
    /// older than 2.31, and then what the worker starts uses the machine's own. One of a kind this worker doesn't
    /// know blocks pushing, and is refused as `Unsupported`, as a value a reader doesn't know is. *Unconfirmed:*
    /// refusing one it blocked for.
    pub(crate) async fn set(&self, credential: GithubCredential) -> Result<(), ApiError> {
        // Only what changes git's config needs a git that reads it, which is asked the first time it matters.
        let needs_no_git = Ok(());
        let git = if matches!(credential, GithubCredential::MachineOwn) {
            &needs_no_git
        } else {
            self.git.get_or_init(|| async { git_reads_the_config(&self.given()).await }).await
        };
        self.apply(credential, git).await
    }

    /// Applies `credential` with a git that reads the config or, as `git` says, doesn't.
    pub(super) async fn apply(&self, credential: GithubCredential, git: &Result<(), String>) -> Result<(), ApiError> {
        let unknown = matches!(credential, GithubCredential::Unknown);
        let handed = match credential {
            GithubCredential::MachineOwn => return self.machine_own().await,
            GithubCredential::Account(credential) => Handed::Account(credential),
            GithubCredential::Blocked { reason } => Handed::Blocked(reason),
            GithubCredential::Unknown => Handed::Blocked(NEWER_HUB.to_string()),
        };
        if let Err(why) = git {
            self.machine_own().await?;
            let why = format!("this worker can't hand on a git credential: {why}");
            return Err(ApiError::new(ErrorCode::Unavailable, why));
        }
        let files = self.files.clone();
        let written = tokio::task::spawn_blocking(move || files.write(&handed)).await.map_err(std::io::Error::other);
        if let Err(e) = written.and_then(|written| written) {
            self.machine_own().await?;
            let why = format!("couldn't write the git credential's files: {e}");
            return Err(ApiError::new(ErrorCode::Unavailable, why));
        }
        self.given.send_replace(Arc::new(self.files.environment(&self.machine)));
        if unknown {
            return Err(ApiError::new(ErrorCode::Unsupported, "this worker doesn't know the kind of git credential"));
        }
        Ok(())
    }

    /// What's started from now on goes without, and so does what was started under one, once the files have gone.
    async fn machine_own(&self) -> Result<(), ApiError> {
        self.given.send_replace(self.machine.clone());
        let files = self.files.clone();
        let removed = tokio::task::spawn_blocking(move || files.remove()).await.map_err(std::io::Error::other);
        removed.and_then(|removed| removed).map_err(|e| {
            ApiError::new(ErrorCode::Unavailable, format!("couldn't remove the git credential's files: {e}"))
        })
    }

    /// Removes the files, as the worker starts and as it exits, so no snapshot of the machine keeps a token, and the
    /// last worker's are gone if it was killed. Only the worker removes them, so a killed worker's stay until the next
    /// worker starts on its instance: the provider clears each run's files, and not these. *Unconfirmed.* What can't
    /// be removed goes in the log.
    pub(crate) fn remove(&self) {
        self.given.send_replace(self.machine.clone());
        if let Err(e) = self.files.remove() {
            warn!("couldn't remove the git credential's files: {e}");
        }
    }
}

async fn git_reads_the_config(given: &Environment) -> Result<(), String> {
    let said = crate::git::version(given).await.map_err(|e| e.message)?;
    reads_the_config(&said)
}
