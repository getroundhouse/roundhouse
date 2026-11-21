use std::time::Duration;

use github::Leaving;
use protocol::model::{GithubRepositoryName, IssueNumber, Reference};

use super::super::Hub;
use crate::github_cache::{Facet, Item};
use crate::jobs;

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
            let cache = self.github_cache.clone();
            if let Err(error) = tokio::task::spawn_blocking(move || cache.keep(read)).await {
                tracing::warn!("keeping what was read from GitHub: {error}");
            }
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
