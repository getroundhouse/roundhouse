//! Choosing a new workspace's name: one no other workspace in its repository has, and git has no branch, folder of
//! branches or directory by.

use std::sync::atomic::Ordering;

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{Repo, RepoId, Workspace};
use protocol::worker::{NamesGitHas, NamesGitHasReply};

use crate::hub::Hub;
use crate::worker_link::WorkerLink;

/// The names a new workspace tries, in turn. One the person chose gets one try. One the hub picks gets a few: the first
/// can be one git has, which the hub learns only once the workspace's worker is up, and one picked after that can
/// still be taken by the time its worktree is made.
pub(super) struct Names {
    chosen: Option<String>,
    left: usize,
    /// The names git has in the repository, as the workspace's worker last said, lowercased here, since git's names
    /// count whatever their case. A name the hub picks skips them, so finding the first taken costs one more try
    /// rather than one for each.
    git_has: Vec<String>,
}

impl Names {
    pub(super) fn new(chosen: Option<String>) -> Self {
        let left = if chosen.is_some() { 1 } else { 8 };
        Self { chosen, left, git_has: Vec::new() }
    }

    /// The next name to try in the repository, or `None` once there are no tries left.
    pub(super) async fn next(&mut self, hub: &Hub, repo_id: &RepoId) -> Result<Option<String>, ApiError> {
        let Some(left) = self.left.checked_sub(1) else { return Ok(None) };
        self.left = left;
        match &self.chosen {
            Some(name) => Ok(Some(name.clone())),
            None => hub.next_name(repo_id, &self.git_has).await.map(Some),
        }
    }
}

pub(super) fn name_taken(name: &str, repo: &Repo) -> ApiError {
    ApiError::new(ErrorCode::NameTaken, format!("{name} is already taken in {}", repo.name))
}

impl Hub {
    async fn next_name(&self, repo_id: &RepoId, git_has: &[String]) -> Result<String, ApiError> {
        let owned = repo_id.clone();
        let mut taken = self.blocking(move |store| store.names_in_repo(&owned)).await?;
        taken.extend_from_slice(git_has);
        let salt = self.name_salt.fetch_add(1, Ordering::Relaxed);
        Ok(crate::names::pick(&taken, salt))
    }

    /// Learns from the workspace's worker which names git has in the repository, so the next name picked skips them
    /// all, and refuses the workspace's own if git has it, whatever its case. A new workspace's worker is always the
    /// hub's own build, so it can say.
    pub(super) async fn refuse_a_name_git_has(
        &self,
        link: &WorkerLink,
        repo: &Repo,
        name: &str,
        names: &mut Names,
    ) -> Result<(), ApiError> {
        let asking = NamesGitHas { repo_name: repo.name.clone(), repo_path: repo.path.clone() };
        let NamesGitHasReply { names: said } = link.call(asking).await?;
        names.git_has = said.iter().map(|had| had.to_ascii_lowercase()).collect();
        if names.git_has.contains(&name.to_ascii_lowercase()) {
            let taken = format!(
                "{name} is taken: git has a branch, a folder of branches, a remote or a directory by it, or a remote's \
                 HEAD, in {}",
                repo.name
            );
            return Err(ApiError::new(ErrorCode::NameTaken, taken));
        }
        Ok(())
    }

    /// Gives a workspace still being made the next of `names` that no other workspace has, and announces it. Fails
    /// with `refused`, what refused the name it has, or the last tried, once there are none left to try.
    pub(super) async fn rename_unready(
        &self,
        workspace: Workspace,
        names: &mut Names,
        mut refused: ApiError,
    ) -> Result<Workspace, ApiError> {
        while let Some(name) = names.next(self, &workspace.repo_id).await? {
            let (id, new_name, branch) = (workspace.id.clone(), name.clone(), crate::names::start_branch(&name));
            match self.blocking(move |store| store.rename_unready_workspace(&id, &new_name, &branch)).await? {
                Some(renamed) => {
                    self.notify(HubNotice::Workspace(renamed.clone()));
                    return Ok(renamed);
                }
                None => refused = ApiError::new(ErrorCode::NameTaken, format!("{name} is already taken")),
            }
        }
        Err(refused)
    }
}
