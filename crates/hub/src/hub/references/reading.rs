use std::time::Duration;

use github::Leaving;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GithubRepositoryName, IssueNumber, Reference, ReferenceAsk, ReferenceRead, WorkspaceId};

use super::super::Hub;
use super::kept::{Key, key};
use crate::jobs;

/// The most one ask can name, which is more than a window shows, so a client that asks for more is wrong.
pub(crate) const MOST_ASKED: usize = 200;

impl Hub {
    /// What GitHub says of each of `asks`, named in a chat of the workspace `id`, in the order asked, from what the hub
    /// keeps where it's fresh, as [`super::Kept`] says, and otherwise read now, as the workspace's pull request is
    /// read, without starting a worker for a token.
    pub(crate) async fn references_read(
        &self,
        id: &WorkspaceId,
        asks: Vec<ReferenceAsk>,
    ) -> Result<Vec<ReferenceRead>, ApiError> {
        if asks.len() > MOST_ASKED {
            let message = format!("{} issues and pull requests asked for at once, over {MOST_ASKED}", asks.len());
            return Err(ApiError::new(ErrorCode::InvalidRequest, message));
        }
        let workspace = self.workspace(id).await?;
        let wanted: Vec<(GithubRepositoryName, IssueNumber)> =
            asks.into_iter().map(|ask| (ask.repository, ask.number)).collect();
        self.read_stale_references(&workspace, &wanted).await;
        Ok(wanted
            .iter()
            .map(|(repository, number)| {
                self.references
                    .read(&key(repository, *number))
                    .unwrap_or_else(|| ReferenceRead::Unread { message: "GitHub didn't say".to_string() })
            })
            .collect())
    }

    /// Reads those of `wanted` that aren't fresh from GitHub, as the workspace's pull request is read, and keeps what
    /// it found, or why it couldn't, and waits for those another ask is reading.
    async fn read_stale_references(
        &self,
        workspace: &protocol::model::Workspace,
        wanted: &[(GithubRepositoryName, IssueNumber)],
    ) {
        let keys: Vec<Key> = wanted.iter().map(|(repository, number)| key(repository, *number)).collect();
        let (mine, theirs) = self.references.claim(&keys);
        if let Some(mine) = mine {
            let mut asks: Vec<(GithubRepositoryName, IssueNumber)> = Vec::new();
            for ask in wanted {
                if mine.keys().contains(&key(&ask.0, ask.1)) && !asks.contains(ask) {
                    asks.push(ask.clone());
                }
            }
            for (key, said) in self.read_from_github(workspace, &asks).await {
                self.references.keep(key, said);
            }
        }
        if !theirs.is_empty() {
            jobs::step("waiting for another read of what chats name");
            for mut read in theirs {
                // It's over once what tells of it is dropped, which is all `changed` waits for.
                let _ = read.changed().await;
            }
        }
    }

    /// What GitHub says of each of `asks`, as the workspace's pull request is read, or why it couldn't be read.
    async fn read_from_github(
        &self,
        workspace: &protocol::model::Workspace,
        asks: &[(GithubRepositoryName, IssueNumber)],
    ) -> Vec<(Key, Result<Option<Reference>, String>)> {
        let all_failed = |why: &str| {
            asks.iter().map(|(repository, number)| (key(repository, *number), Err(why.to_string()))).collect()
        };
        let reads_as = match self.reads_as(workspace) {
            Ok(reads_as) => reads_as,
            Err(error) => return all_failed(&error.message),
        };
        let token = match self.token_of(&reads_as, Duration::ZERO).await {
            Ok(token) => token,
            Err(error) => return all_failed(&error.message),
        };
        if let Err(error) = self.may_read_unasked(&reads_as) {
            return all_failed(&error.message);
        }
        jobs::step("waiting for GitHub to answer for what chats name");
        let read = self.github.read_references(&token, asks, Leaving::ForOthers).await;
        self.github_said(&reads_as, read.rate_limit, read.refused);
        (asks.iter().zip(read.answers))
            .map(|((repository, number), answer)| (key(repository, *number), answer.map_err(|error| error.message)))
            .collect()
    }
}
