//! A read of many issues and pull requests at once, as one token, each to its own answer, so one GitHub has nothing
//! for, or fails on, fails no other.

use std::collections::HashMap;

use protocol::client::ApiError;
use protocol::model::{GithubRepositoryName, GithubToken, IssueNumber, Reference};
use serde_json::Value;

use super::parse::reference;
use super::query::{Grouped, alias, grouped, number_alias, query};
use crate::pull_requests::{Spent, failed, incomplete};
use crate::{Github, Leaving, RateLimit};

/// What a read found for each of its asks.
#[derive(Debug)]
pub struct ReferencesRead {
    /// One answer for each ask, in the order asked: the issue or pull request, or `None` where GitHub says there's no
    /// such one the token can see, as in a repository that isn't there or that it can't see, or why it couldn't be
    /// read, such as GitHub failing, limiting the hub, or answering in part.
    pub answers: Vec<Result<Option<Reference>, ApiError>>,
    /// What GitHub last said is left of its limit for the token, if it said.
    pub rate_limit: Option<RateLimit>,
    /// Whether GitHub refused the token.
    pub refused: bool,
}

/// What an ask comes to before it's read from GitHub's data: what an error GitHub gave about it said, if one did.
enum Said {
    /// It isn't there, or the token can't see it.
    Missing,
    Failed(ApiError),
}

impl Github {
    /// Reads each of `asks`, a repository and a number, as `token`, [`ASKS_A_QUERY`](super::query::ASKS_A_QUERY) to
    /// a query, each repository's numbers together and each ask once, so the same one asked twice has the same answer
    /// twice. Stops asking once GitHub's limit for the token is down to what `leaving` leaves.
    pub async fn read_references(
        &self,
        token: &GithubToken,
        asks: &[(GithubRepositoryName, IssueNumber)],
        leaving: Leaving,
    ) -> ReferencesRead {
        let mut spent = Spent::new(leaving);
        let mut found: HashMap<(&GithubRepositoryName, IssueNumber), Result<Option<Reference>, ApiError>> =
            HashMap::new();
        for batch in grouped(asks) {
            // Collected before the query, since a lazy iterator held across it would keep the read from being sent
            // to another thread.
            let asked: Vec<(&GithubRepositoryName, IssueNumber)> = batch
                .iter()
                .flat_map(|(repository, numbers)| numbers.iter().map(|number| (*repository, *number)))
                .collect();
            let began_ms = jiff::Timestamp::now().as_millisecond();
            match self.spend(token, &query(&batch), &mut spent).await {
                Ok(answer) => found.extend(asked.into_iter().zip(taken(&answer, &batch, began_ms))),
                Err(error) => found.extend(asked.into_iter().map(|ask| (ask, Err(error.clone())))),
            }
        }
        let answers = asks
            .iter()
            .map(|(repository, number)| found.get(&(repository, *number)).cloned().unwrap_or_else(|| Err(incomplete())))
            .collect();
        ReferencesRead { answers, rate_limit: spent.rate_limit, refused: spent.refused }
    }
}

/// Each of `batch`'s answers from `answer`, repository by repository and number by number, as `batch` has them, with
/// the errors GitHub gave about each by the aliases their paths start with. GitHub answers one that isn't there, or
/// that the token can't see, with `null` and an error of type `NOT_FOUND` whose path is its alias, a repository's or a
/// number's. Any other error about one fails it alone, and one whose path names none of them, such as one about the
/// query itself, fails them all. What's found was read by a read that began at `read_at_ms`.
pub(super) fn taken(answer: &Value, batch: &Grouped<'_>, read_at_ms: i64) -> Vec<Result<Option<Reference>, ApiError>> {
    let mut said: HashMap<(usize, Option<IssueNumber>), Said> = HashMap::new();
    for error in answer["errors"].as_array().into_iter().flatten() {
        let path: Vec<&str> = error["path"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let Some(at) = (0..batch.len()).find(|at| path.first() == Some(&alias(*at).as_str())) else {
            let failure = Err(failed(error));
            return batch.iter().flat_map(|(_, numbers)| numbers.iter().map(|_| failure.clone())).collect();
        };
        let number = path.get(1).and_then(|part| batch[at].1.iter().find(|number| number_alias(**number) == *part));
        let whole = path.len() == if number.is_some() { 2 } else { 1 };
        if whole && error["type"] == "NOT_FOUND" {
            said.entry((at, number.copied())).or_insert(Said::Missing);
        } else {
            said.insert((at, number.copied()), Said::Failed(failed(error)));
        }
    }
    let mut answers = Vec::new();
    for (at, (_, numbers)) in batch.iter().enumerate() {
        let repository = &answer["data"][alias(at)];
        for number in numbers {
            let answered = match said.get(&(at, None)).or_else(|| said.get(&(at, Some(*number)))) {
                Some(Said::Missing) => Ok(None),
                Some(Said::Failed(error)) => Err(error.clone()),
                None => {
                    let node = &repository[number_alias(*number)];
                    if node.is_null() { Err(incomplete()) } else { reference(repository, node, read_at_ms).map(Some) }
                }
            };
            answers.push(answered);
        }
    }
    answers
}
