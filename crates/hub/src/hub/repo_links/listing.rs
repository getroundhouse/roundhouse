//! The repositories the hub's GitHub accounts can see, together, which a client lists for a person to add a project
//! from, searched and a page at a time.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::github_repositories::{GithubRepositories, GithubRepositoryList, GithubRepositorySeen};
use protocol::client::{ApiError, ErrorCode};
use tokio::time::Instant;
use tracing::warn;

use crate::hub::{Hub, off_thread};
use crate::store::now_ms;

/// The repositories a page has at most.
const A_PAGE: usize = 50;

/// The pages read of one account at most, a thousand repositories, so an account that sees a whole enterprise's
/// doesn't hold a person up for minutes. *Unconfirmed.*
const PAGES_AT_MOST: usize = 10;

/// How long what was read stays the answer, so a person searching as they type asks GitHub once, and a repository
/// made on GitHub shows within a minute. *Unconfirmed.*
const FRESH_FOR: Duration = Duration::from_mins(1);

/// What was last read of every account's repositories, and when, which only the list reads.
#[derive(Default)]
pub(crate) struct SeenLately(Mutex<Option<(Instant, Arc<Vec<GithubRepositorySeen>>)>>);

impl Hub {
    /// The page `asked` asks for of the repositories every connected account can see, as `GithubRepositoryList` says.
    /// Refused with `Unavailable` with no account to ask as, or when GitHub fails for every one, and with
    /// `InvalidRequest` for a start no page gave.
    pub(crate) async fn github_repository_list(
        &self,
        asked: GithubRepositoryList,
    ) -> Result<GithubRepositories, ApiError> {
        let start = match asked.after.as_deref() {
            Some(after) => after.parse::<usize>().map_err(|_| {
                ApiError::new(ErrorCode::InvalidRequest, "that isn't where a page of repositories starts")
            })?,
            None => 0,
        };
        let seen = self.every_account_sees().await?;
        let search = asked.search.trim().to_lowercase();
        let matching: Vec<&GithubRepositorySeen> =
            seen.iter().filter(|seen| seen.repository.as_str().to_lowercase().contains(&search)).collect();
        let repositories = matching.iter().skip(start).take(A_PAGE).map(|seen| (*seen).clone()).collect();
        let after = (matching.len() > start + A_PAGE).then(|| (start + A_PAGE).to_string());
        Ok(GithubRepositories { repositories, after })
    }

    /// Every repository the connected accounts can see, each once, by owner and name, as read within [`FRESH_FOR`].
    async fn every_account_sees(&self) -> Result<Arc<Vec<GithubRepositorySeen>>, ApiError> {
        if let Some((read_at, seen)) = &*self.repositories_seen.0.lock()
            && read_at.elapsed() < FRESH_FOR
        {
            return Ok(Arc::clone(seen));
        }
        let logins = Arc::clone(&self.github_logins);
        let readers = off_thread(move || logins.readers(now_ms())).await?;
        if readers.is_empty() {
            let message = "no GitHub account is connected to list repositories as; connect one in Settings first";
            return Err(ApiError::new(ErrorCode::Unavailable, message));
        }
        let mut seen: BTreeMap<String, GithubRepositorySeen> = BTreeMap::new();
        let mut failed = None;
        for (username, token) in &readers {
            let mut after = None;
            for _ in 0..PAGES_AT_MOST {
                let page = match self.github.repositories_seen(token, after.as_deref()).await {
                    Ok(page) => page,
                    Err(e) => {
                        warn!(%username, "listing the repositories a GitHub account can see: {}", e.message);
                        failed = Some(e);
                        break;
                    }
                };
                for github::Seen { found, private } in page.repositories {
                    let key = format!("{}\0{}", found.said.repository.as_str().to_lowercase(), found.id);
                    let entry = seen.entry(key).or_insert_with(|| GithubRepositorySeen {
                        id: found.id,
                        repository: found.said.repository,
                        private,
                        seen_by: Vec::new(),
                    });
                    entry.seen_by.push(username.clone());
                }
                after = page.next;
                if after.is_none() {
                    break;
                }
            }
        }
        if seen.is_empty()
            && let Some(e) = failed
        {
            return Err(ApiError::new(ErrorCode::Unavailable, e.message));
        }
        let seen = Arc::new(seen.into_values().collect::<Vec<_>>());
        *self.repositories_seen.0.lock() = Some((Instant::now(), Arc::clone(&seen)));
        Ok(seen)
    }
}
