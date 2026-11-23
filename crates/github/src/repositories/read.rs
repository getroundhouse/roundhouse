//! A read of many repositories at once, as one token, each to its own answer, so one GitHub has nothing for, or fails
//! on, fails no other.

use protocol::client::ApiError;
use protocol::model::{GithubRepositoryId, GithubRepositoryName, GithubRepositorySaid, GithubToken, RepoUrl};
use serde_json::Value;

use super::query::{ASKS_A_QUERY, RepositoryAsk, alias, query};
use crate::pull_requests::{Spent, failed, incomplete, string};
use crate::{Github, Leaving, RateLimit};

/// A repository GitHub has: its ID, and what it says of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryFound {
    pub id: GithubRepositoryId,
    pub said: GithubRepositorySaid,
}

/// What a read found for each of its asks.
#[derive(Debug)]
pub struct RepositoriesRead {
    /// One answer for each ask, in the order asked: the repository, or `None` where GitHub says there's none the token
    /// can see, as for one that isn't there or is private and not the token's, or why it couldn't be read, such as
    /// GitHub failing, limiting the hub, or answering in part.
    pub answers: Vec<Result<Option<RepositoryFound>, ApiError>>,
    /// What GitHub last said is left of its limit for the token, if it said.
    pub rate_limit: Option<RateLimit>,
    /// Whether GitHub refused the token.
    pub refused: bool,
}

impl Github {
    /// Reads each of `asks` as `token`, [`ASKS_A_QUERY`] to a query. Stops asking once GitHub's limit for the token is
    /// down to what `leaving` leaves.
    pub async fn read_repositories(
        &self,
        token: &GithubToken,
        asks: &[RepositoryAsk],
        leaving: Leaving,
    ) -> RepositoriesRead {
        let mut spent = Spent::new(leaving);
        let mut answers = Vec::with_capacity(asks.len());
        for batch in asks.chunks(ASKS_A_QUERY) {
            match self.spend(token, &query(batch), &mut spent).await {
                Ok(answer) => answers.extend(taken(&answer, batch.len())),
                Err(error) => answers.extend(batch.iter().map(|_| Err(error.clone()))),
            }
        }
        RepositoriesRead { answers, rate_limit: spent.rate_limit, refused: spent.refused }
    }
}

/// Each of a batch of `asked` answers from `answer`, by its alias, with the errors GitHub gave about each. GitHub
/// answers one that isn't there, or that the token can't see, with `null` and an error of type `NOT_FOUND` whose path
/// is its alias. Any other error about one fails it alone, and one whose path names none of them, such as one about
/// the query itself, fails them all.
pub(super) fn taken(answer: &Value, asked: usize) -> Vec<Result<Option<RepositoryFound>, ApiError>> {
    let mut said: Vec<Option<Result<Option<RepositoryFound>, ApiError>>> = vec![None; asked];
    for error in answer["errors"].as_array().into_iter().flatten() {
        let path: Vec<&str> = error["path"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let Some(at) = (0..asked).find(|at| path.first() == Some(&alias(*at).as_str())) else {
            return vec![Err(failed(error)); asked];
        };
        let missing = path.len() == 1 && error["type"] == "NOT_FOUND";
        if missing {
            said[at].get_or_insert(Ok(None));
        } else {
            said[at] = Some(Err(failed(error)));
        }
    }
    said.into_iter().enumerate().map(|(at, said)| said.unwrap_or_else(|| found(&answer["data"][alias(at)]))).collect()
}

/// The repository `node` is, as GitHub answered for it. An object with nothing in it is what `node(id:)` answers for
/// an ID that's something other than a repository, so it's none.
pub(super) fn found(node: &Value) -> Result<Option<RepositoryFound>, ApiError> {
    if node.is_null() {
        return Err(incomplete());
    }
    if node.as_object().is_some_and(serde_json::Map::is_empty) {
        return Ok(None);
    }
    let address = |key: &str| string(node, key).and_then(|url| RepoUrl::try_from(url).map_err(|_| incomplete()));
    let repository = GithubRepositoryName::try_from(string(node, "nameWithOwner")?).map_err(|_| incomplete())?;
    let said = GithubRepositorySaid { repository, https: address("url")?, ssh: address("sshUrl")? };
    Ok(Some(RepositoryFound { id: string(node, "id")?.into(), said }))
}
