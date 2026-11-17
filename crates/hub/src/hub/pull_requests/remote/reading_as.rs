//! Whose token the hub reads a workspace's pull requests with: the default account's, wherever the workspace runs, or
//! while each machine's own login is the default, the one the workspace's provider's `gh` handed the hub.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GithubAccountId, GithubRepositoryName, GithubToken, ProviderName, RepoUrl, Workspace};
use tracing::info;

use std::time::Duration;

use super::{Arc, Hub};
use crate::github_logins::DefaultCredential;
use crate::hub::found_github_logins::FOUND_WITHIN;
use crate::worker_link::WorkerLink;

/// Whose token a read goes as.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::hub) enum ReadsAs {
    /// The default account's.
    Account(GithubAccountId),
    /// The machine's own, of the provider's machines, while that's the default.
    MachineOwn(ProviderName),
}

impl std::fmt::Display for ReadsAs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Account(account) => write!(f, "the default account, {account}"),
            Self::MachineOwn(provider) => write!(f, "the own gh login of {provider}'s machines"),
        }
    }
}

impl Hub {
    /// Who the workspace's pull requests are read as, as the default says now. While the default blocks, as once it
    /// was disconnected, a read fails, as one that may work later, once it's reconnected or another is chosen, so a
    /// wait goes on waiting.
    pub(in crate::hub) fn reads_as(&self, workspace: &Workspace) -> Result<ReadsAs, ApiError> {
        match &*self.git_credential().borrow() {
            DefaultCredential::Account { account, .. } => Ok(ReadsAs::Account(account.clone())),
            DefaultCredential::MachineOwn => Ok(ReadsAs::MachineOwn(workspace.provider.clone())),
            DefaultCredential::Blocked { reason, .. } => Err(ApiError::new(ErrorCode::Worker, reason.clone())),
        }
    }

    /// The token to read as `reads_as` with: the default account's, while it's still the default, or the one the
    /// provider's machines' `gh` handed the hub, waiting `within` for a worker asked as it linked to hand it over.
    /// Fails when it handed none, as when it's logged out, and when the default is another now, each as a read that
    /// may work later, since `gh` can log in, and the default can change back.
    pub(in crate::hub) async fn token_of(&self, reads_as: &ReadsAs, within: Duration) -> Result<GithubToken, ApiError> {
        match reads_as {
            ReadsAs::Account(wanted) => match &*self.git_credential().borrow() {
                DefaultCredential::Account { account, credential, .. } if account == wanted => {
                    Ok(credential.token.clone())
                }
                DefaultCredential::Account { .. }
                | DefaultCredential::MachineOwn
                | DefaultCredential::Blocked { .. } => {
                    Err(ApiError::new(ErrorCode::Worker, "the default GitHub account changed as it was read"))
                }
            },
            ReadsAs::MachineOwn(provider) => match self.found_tokens.of(provider, within).await {
                Some(found) => Ok(found.token),
                None => Err(ApiError::new(
                    ErrorCode::Worker,
                    format!(
                        "gh on {provider}'s machines handed the hub no token to read GitHub with, as when it isn't \
                         logged in. Log it in there, with gh auth login, or add an account in Settings"
                    ),
                )),
            },
        }
    }

    /// Who the workspace's pull requests are read as, with the token, as [`Hub::reads_as`] says, for an agent, whose
    /// workspace's worker, which serves its tools, is asked for its machines' token again when it has none, as
    /// [`Hub::machine_token`] says.
    pub(in crate::hub) async fn reading_as(
        self: &Arc<Self>,
        workspace: &Workspace,
    ) -> Result<(ReadsAs, GithubToken), ApiError> {
        let reads_as = self.reads_as(workspace)?;
        let token = match &reads_as {
            ReadsAs::Account(_) => self.token_of(&reads_as, Duration::ZERO).await?,
            // Its worker is looked for only when there's no token, since finding it keeps it running.
            ReadsAs::MachineOwn(provider) => match self.token_of(&reads_as, Duration::ZERO).await {
                Ok(token) => token,
                Err(_) => self.machine_token(provider, self.existing_worker(&workspace.id).as_ref()).await?,
            },
        };
        Ok((reads_as, token))
    }

    /// The token `provider`'s machines' own `gh` handed the hub. When it handed none, as a `gh` logged out, an older
    /// worker, or one whose token GitHub refused, `link`'s worker, of `provider`, is asked again, as `gh` may have
    /// logged in since, and waited for as long as the hub waits for a worker to say who its `gh` is. With no worker to
    /// ask, it fails at once. So a read that
    /// needs the token asks for it as often as it's read, which its backing off keeps to now and then.
    pub(in crate::hub) async fn machine_token(
        self: &Arc<Self>,
        provider: &ProviderName,
        link: Option<&Arc<WorkerLink>>,
    ) -> Result<GithubToken, ApiError> {
        let reads_as = ReadsAs::MachineOwn(provider.clone());
        if let Ok(token) = self.token_of(&reads_as, Duration::ZERO).await {
            return Ok(token);
        }
        if self.found_tokens.forget_none(provider) {
            info!(%provider, "asking again for the token of gh on {provider}'s machines, which handed none");
            self.github_logins_asked.forget(provider);
        }
        // Without a worker to ask, only one being asked already could hand it over, which needn't be waited for.
        let Some(link) = link else { return self.token_of(&reads_as, Duration::ZERO).await };
        self.find_github_login(provider.clone(), link).await;
        self.token_of(&reads_as, FOUND_WITHIN).await
    }

    /// GitHub refused the token `reads_as` had, so a machine's own is forgotten, for the next read that needs it to ask
    /// for it again, as [`Hub::machine_token`] says.
    pub(in crate::hub) fn reading_token_refused(&self, reads_as: &ReadsAs) {
        let ReadsAs::MachineOwn(provider) = reads_as else { return };
        self.found_tokens.forget(provider);
        self.github_logins_asked.forget(provider);
    }

    /// Which repository on GitHub the workspace's is, as its record's address names it.
    pub(in crate::hub) async fn on_github(&self, workspace: &Workspace) -> Result<GithubRepositoryName, ApiError> {
        let repo = self.repo_of(workspace).await?;
        repo.origin.url().and_then(RepoUrl::on_github).ok_or_else(|| {
            let message = "the workspace's repository has no address on github.com the hub knows of, so the hub can't \
                           read its pull requests";
            ApiError::new(ErrorCode::Unavailable, message)
        })
    }
}
