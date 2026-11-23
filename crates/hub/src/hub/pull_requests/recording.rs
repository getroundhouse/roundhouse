//! Recording on a workspace which pull request a read found, or that it has none, which the workspace's record keeps
//! across restarts, so what's known of it outlasts what the cache keeps.

use super::super::Hub;
use super::unseen::OwnUnread;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    BranchName, GithubRepositoryName, IssueNumber, OwnPullRequest, PullRequestFound, PullRequestLink,
    PullRequestLinked, Workspace,
};

impl Hub {
    /// Records which pull request a read found, with the branch it found it on, and where it can't be read at all and
    /// none was found before, that there's none. GitHub showing no repository to look in says nothing of whether
    /// there's one, so then a pull request found before is kept, and none found before is no longer known, as before
    /// the hub first looked, so no app offers to create one there. Returns what it found, or that there's none where it
    /// recorded that, and the workspace if that changed it.
    pub(super) async fn record(
        &self,
        workspace: &Workspace,
        read: &Result<Found, OwnUnread>,
    ) -> Result<(Option<PullRequestFound>, Option<Workspace>), ApiError> {
        let found = match read {
            Ok(found) => found.clone(),
            // What was found before is kept, on the branch it was found on, since nothing new was found.
            // But not where GitHub last showed no repository, where none is no longer known, as below, so a read that
            // fails after one that couldn't see it doesn't say there's none again.
            Err(OwnUnread::Failed(e))
                if e.code == ErrorCode::Unavailable
                    && workspace.pull_request_link.is_none()
                    && self.unseen_of(&workspace.id).is_none() =>
            {
                Found::nothing(workspace.pull_request_branch.clone())
            }
            Err(OwnUnread::Unseen { .. }) if workspace.pull_request_link.as_ref().is_some_and(|l| l.link.is_none()) => {
                let id = workspace.id.clone();
                let changed = self.blocking(move |store| store.set_workspace_pull_request(&id, None, None)).await?;
                return Ok((None, changed));
            }
            Err(OwnUnread::Failed(_) | OwnUnread::Unseen { .. }) => return Ok((None, None)),
        };
        let Some(linked) = found.linked() else { return Ok((None, None)) };
        let (id, on) = (workspace.id.clone(), found.on);
        let changed =
            self.blocking(move |store| store.set_workspace_pull_request(&id, Some(&linked), on.as_ref())).await?;
        Ok((Some(found.read), changed))
    }
}

/// What a read found, the repository it's in, and the branch it found it on.
#[derive(Clone)]
pub(super) struct Found {
    pub(super) read: PullRequestFound,
    /// The repository the pull request found merges into, `None` where none was.
    into: Option<GithubRepositoryName>,
    pub(super) on: Option<BranchName>,
}

impl Found {
    /// `pull_request`, into `into`, found on `on`.
    pub(super) fn of(pull_request: OwnPullRequest, into: GithubRepositoryName, on: Option<BranchName>) -> Self {
        Self { read: PullRequestFound { pull_request: Some(pull_request) }, into: Some(into), on }
    }

    /// None, on `on`, as for a branch pushed nowhere.
    pub(super) fn nothing(on: Option<BranchName>) -> Self {
        Self { read: PullRequestFound { pull_request: None }, into: None, on }
    }

    /// Which pull request it found, as the workspace's record keeps it. `None` for one whose number no pull request
    /// can have, which GitHub doesn't give.
    fn linked(&self) -> Option<PullRequestLinked> {
        let Some(own) = &self.read.pull_request else { return Some(PullRequestLinked { link: None }) };
        let link = PullRequestLink {
            repository: self.into.clone()?,
            number: IssueNumber::try_from(own.pull_request.number.get()).ok()?,
            url: own.pull_request.said().about.url.clone(),
            uncommitted: own.local.uncommitted,
        };
        Some(PullRequestLinked { link: Some(link) })
    }
}
