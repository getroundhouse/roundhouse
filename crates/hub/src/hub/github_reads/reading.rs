use std::time::Duration;

use github::{Leaving, PullRequestsAsk, PullRequestsRead, SummaryRead};
use protocol::model::{GithubRepositoryName, GithubToken, IssueNumber, Reference};

use super::super::Hub;
use crate::github_cache::{Facet, Item};
use crate::jobs;

/// How long a branch's rules for required checks, which change seldom, are taken as they were read, rather than read
/// again with every read of pull requests into it, so a change to them reaches waits within this long. *Unconfirmed.*
pub(in crate::hub) const RULES_FRESH_FOR: Duration = Duration::from_mins(10);

impl Hub {
    /// Reads from GitHub the summaries of those of `asks` the cache has none of as fresh as `fresh_for`, and keeps what
    /// it found, or why it couldn't, and waits for those another ask is reading, so each is read once however many
    /// ask for it.
    pub(super) async fn read_summaries(&self, asks: &[(GithubRepositoryName, IssueNumber)], fresh_for: Duration) {
        let items: Vec<Item> = asks.iter().map(|(repository, number)| Item::new(repository, *number)).collect();
        let (mine, theirs) = self.github_cache.claim(&items, Facet::Summary, fresh_for);
        if let Some(mine) = mine {
            let mut reading: Vec<(GithubRepositoryName, IssueNumber)> = Vec::new();
            for (ask, item) in asks.iter().zip(&items) {
                if mine.items().contains(item) && !reading.iter().any(|read| Item::new(&read.0, read.1) == *item) {
                    reading.push(ask.clone());
                }
            }
            let read = self.summaries_from_github(&reading).await;
            self.keep_summaries(read).await;
            // Only now, so an ask that waited on these finds what was kept.
            drop(mine);
        }
        if !theirs.is_empty() {
            jobs::step("waiting for another read of the same issues and pull requests");
            for mut read in theirs {
                // It's over once what tells of it is dropped, which is all `changed` waits for.
                let _ = read.changed().await;
            }
        }
    }

    /// Reads `asks` as `token`, leaving what `leaving` leaves of GitHub's limit, as [`github::Github::read_pull_requests`]
    /// does, but for the rules of their branches the cache has read within [`RULES_FRESH_FOR`], which it gives each ask
    /// as known. Keeps the summaries and rules it read.
    pub(in crate::hub) async fn read_pull_requests(
        &self,
        token: &GithubToken,
        mut asks: Vec<PullRequestsAsk>,
        leaving: Leaving,
    ) -> PullRequestsRead {
        for ask in &mut asks {
            ask.known_rules = self.github_cache.rules_of(&ask.repository, RULES_FRESH_FOR);
        }
        let mut read = self.github.read_pull_requests(token, asks, leaving).await;
        self.keep_read_summaries(std::mem::take(&mut read.summaries)).await;
        let rules: Vec<_> = std::mem::take(&mut read.rules)
            .into_iter()
            .map(|read| (read.repository, read.branch, read.rules))
            .collect();
        if !rules.is_empty() {
            let cache = self.github_cache.clone();
            if let Err(error) = tokio::task::spawn_blocking(move || cache.keep_rules(rules)).await {
                tracing::warn!("keeping the rules read from GitHub: {error}");
            }
        }
        read
    }

    /// Keeps the summaries a read of pull requests found as it read the rest of them, as a read of the summaries alone
    /// would, so a chip of one says what the bar's read, or a wait's, found.
    async fn keep_read_summaries(&self, summaries: Vec<SummaryRead>) {
        let read = summaries
            .into_iter()
            .filter_map(|SummaryRead { repository, number, summary }| {
                let number = IssueNumber::try_from(number.get()).ok()?;
                Some((Item::new(&repository, number), summary.map(Some).map_err(|error| error.message)))
            })
            .collect();
        self.keep_summaries(read).await;
    }

    /// Keeps what GitHub said of each summary, as [`crate::github_cache::GithubCache::keep`] says.
    async fn keep_summaries(&self, read: Vec<(Item, Result<Option<Reference>, String>)>) {
        if read.is_empty() {
            return;
        }
        let cache = self.github_cache.clone();
        if let Err(error) = tokio::task::spawn_blocking(move || cache.keep(read)).await {
            tracing::warn!("keeping what was read from GitHub: {error}");
        }
    }

    /// What GitHub says of the summary of each of `asks`, or why it couldn't be read.
    async fn summaries_from_github(
        &self,
        asks: &[(GithubRepositoryName, IssueNumber)],
    ) -> Vec<(Item, Result<Option<Reference>, String>)> {
        let all_failed = |why: &str| {
            asks.iter().map(|(repository, number)| (Item::new(repository, *number), Err(why.to_string()))).collect()
        };
        let reads_as = match self.reads_as() {
            Ok(reads_as) => reads_as,
            Err(error) => return all_failed(&error.message),
        };
        let token = match self.token_of(&reads_as) {
            Ok(token) => token,
            Err(error) => return all_failed(&error.message),
        };
        if let Err(error) = self.may_read_unasked(&reads_as) {
            return all_failed(&error.message);
        }
        jobs::step("waiting for GitHub to answer for issues and pull requests");
        let read = self.github.read_references(&token, asks, Leaving::ForOthers).await;
        self.github_said(&reads_as, read.rate_limit);
        (asks.iter().zip(read.answers))
            .map(|((repository, number), answer)| {
                (Item::new(repository, *number), answer.map_err(|error| error.message))
            })
            .collect()
    }
}
