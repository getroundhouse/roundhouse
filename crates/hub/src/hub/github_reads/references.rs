use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GithubRepositoryName, IssueNumber, ReferenceAsk, ReferenceRead, WorkspaceId};

use super::super::Hub;
use crate::github_cache::Item;

/// The most one ask can name, which is more than a window shows, so a client that asks for more is wrong.
pub(crate) const MOST_ASKED: usize = 200;

/// How old what the hub found of one can be when a client asks for it, so one on screen in a client that asks every
/// minute says it's merged within a minute or two of it merging. A read that failed is tried again no sooner, so a
/// GitHub that's down or slow is asked no more often than one that answers. *Unconfirmed.*
pub(super) const FRESH_FOR: Duration = Duration::from_secs(60);

impl Hub {
    /// What GitHub says of each of `asks`, named in a chat of the workspace `id`, in the order asked, from the cache
    /// where it's no older than [`FRESH_FOR`], and otherwise read now.
    pub(crate) async fn references_read(
        &self,
        id: &WorkspaceId,
        asks: Vec<ReferenceAsk>,
    ) -> Result<Vec<ReferenceRead>, ApiError> {
        if asks.len() > MOST_ASKED {
            let message = format!("{} issues and pull requests asked for at once, over {MOST_ASKED}", asks.len());
            return Err(ApiError::new(ErrorCode::InvalidRequest, message));
        }
        // Only to refuse a workspace the hub has no record of.
        self.workspace(id).await?;
        let wanted: Vec<(GithubRepositoryName, IssueNumber)> =
            asks.into_iter().map(|ask| (ask.repository, ask.number)).collect();
        self.read_summaries(&wanted, FRESH_FOR).await;
        Ok(wanted
            .iter()
            .map(|(repository, number)| {
                self.github_cache
                    .summary(&Item::new(repository, *number))
                    .unwrap_or_else(|| ReferenceRead::Unread { message: "GitHub didn't say".to_string() })
            })
            .collect())
    }
}
