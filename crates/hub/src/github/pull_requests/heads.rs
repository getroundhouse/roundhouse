//! Finding the pull requests whose head is a branch on GitHub, as a workspace's are: the pull requests of the branch
//! its worktree pushes to, which its upstream names. A repository and a branch name are unique on GitHub, where local
//! names repeat across machines, so the branch on GitHub is what says which pull requests a workspace's work goes to.
//! Many branches go in one query, each under an alias of its own, as the waits' reads do.

use protocol::client::ApiError;
use protocol::model::{BranchName, CommitId, GithubRepositoryName, GithubToken, PrState, PullRequestNumber};
use protocol::worker::RateLimit;
use serde_json::Value;

use super::graphql::NoAnswer;
use super::query::quote;
use super::rounds::{Leaving, Spent};
use super::said::{failed, incomplete};
use crate::github::Github;

/// The branches one query asks after at most. Each asks for no more than [`NEWEST`] pull requests, with only what
/// tells them apart, so a query of these costs a point or two.
const ASKS_A_QUERY: usize = 50;

/// The pull requests asked for of each branch, the newest first. A branch name used again once its pull request has
/// merged and the branch is gone has the old one too, so more than one, but a name used again more than this many
/// times leaves the oldest out.
const NEWEST: usize = 5;

/// A branch on GitHub whose pull requests are asked for: those in `repository`, the one they merge into, whose head
/// is `branch` of `from`, which is `repository` itself unless the branch is pushed to a fork.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeadsAsk {
    pub(crate) repository: GithubRepositoryName,
    pub(crate) from: GithubRepositoryName,
    pub(crate) branch: BranchName,
}

/// A pull request whose head is the branch asked after, with what tells it apart from another of the same head.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Headed {
    pub(crate) number: PullRequestNumber,
    pub(crate) state: PrState,
    /// Its newest commit.
    pub(crate) head: CommitId,
}

/// What a query found of each ask, in the order they were asked, the newest pull request first.
#[derive(Debug)]
pub(crate) struct HeadsRead {
    pub(crate) answers: Vec<Result<Vec<Headed>, ApiError>>,
    /// What GitHub last said is left of its limit for the token, if it said.
    pub(crate) rate_limit: Option<RateLimit>,
    /// Whether GitHub refused the token.
    pub(crate) refused: bool,
}

impl Github {
    /// The pull requests of each of `asks`, as `token`, [`ASKS_A_QUERY`] to a query, each to its own answer, so one
    /// GitHub has nothing for, such as a repository it doesn't show the token, fails no other. Stops asking once
    /// GitHub's limit for the token is down to what `leaving` leaves.
    pub(crate) async fn read_heads(&self, token: &GithubToken, asks: &[HeadsAsk], leaving: Leaving) -> HeadsRead {
        let mut spent = Spent { rate_limit: None, refused: false, leaving };
        let mut answers = Vec::with_capacity(asks.len());
        for (batch, first) in asks.chunks(ASKS_A_QUERY).zip((0..).step_by(ASKS_A_QUERY)) {
            let answered = if let Some(exhausted) = spent.exhausted() {
                Err(exhausted)
            } else {
                let answered = self.graphql(token, &query(batch, first)).await;
                spent.took(answered.as_ref().ok());
                spent.refused |= matches!(answered, Err(NoAnswer::Refused));
                answered.map_err(|no| no.error())
            };
            match answered {
                Ok(answer) => answers.extend(taken(&answer, batch, first)),
                Err(error) => answers.extend(batch.iter().map(|_| Err(error.clone()))),
            }
        }
        HeadsRead { answers, rate_limit: spent.rate_limit, refused: spent.refused }
    }
}

/// The query asking after `batch`, whose asks are numbered from `first`.
fn query(batch: &[HeadsAsk], first: usize) -> String {
    let asked: Vec<String> = batch
        .iter()
        .zip(first..)
        .map(|(ask, at)| {
            let name = ask.repository.as_str().split_once('/').map_or("", |(_, name)| name);
            format!(
                "r{at}: repository(owner:{},name:{}) {{ h: pullRequests(headRefName:{},first:{NEWEST},\
                 orderBy:{{field:CREATED_AT,direction:DESC}}) {{ nodes {{ number state headRefOid \
                 headRepository {{ nameWithOwner }} }} }} }}",
                quote(ask.repository.owner()),
                quote(name),
                quote(ask.branch.as_str()),
            )
        })
        .collect();
    format!("query {{ {} rateLimit {{ limit remaining resetAt }} }}", asked.join(" "))
}

/// Each ask's answer from `answer`, by its alias, with the errors GitHub gave about it. One whose path names no ask,
/// as one about the query itself, fails them all.
fn taken(answer: &Value, batch: &[HeadsAsk], first: usize) -> Vec<Result<Vec<Headed>, ApiError>> {
    let mut errors: Vec<Option<ApiError>> = batch.iter().map(|_| None).collect();
    for error in answer["errors"].as_array().into_iter().flatten() {
        let alias = error["path"].as_array().and_then(|path| path.first()).and_then(Value::as_str);
        let at = alias.and_then(|alias| alias.strip_prefix('r')).and_then(|at| at.parse::<usize>().ok());
        match at.and_then(|at| at.checked_sub(first)).filter(|at| *at < batch.len()) {
            Some(at) => errors[at] = Some(failed(error)),
            None => return batch.iter().map(|_| Err(failed(error))).collect(),
        }
    }
    batch
        .iter()
        .zip(first..)
        .zip(errors)
        .map(|((ask, at), error)| match error {
            Some(error) => Err(error),
            None => headed(&answer["data"][format!("r{at}")]["h"], &ask.from),
        })
        .collect()
}

/// The pull requests in `found` whose head is in `from`, newest first. One whose head repository is gone, as a fork
/// deleted since, has no head to be the branch asked after.
fn headed(found: &Value, from: &GithubRepositoryName) -> Result<Vec<Headed>, ApiError> {
    let nodes = found["nodes"].as_array().ok_or_else(incomplete)?;
    let mut headed = Vec::new();
    for node in nodes {
        let of = node["headRepository"]["nameWithOwner"].as_str();
        if !of.is_some_and(|of| of.eq_ignore_ascii_case(from.as_str())) {
            continue;
        }
        let number =
            node["number"].as_u64().and_then(|n| PullRequestNumber::try_from(n).ok()).ok_or_else(incomplete)?;
        let state = serde_json::from_value(node["state"].clone()).unwrap_or(PrState::Unknown);
        let head = node["headRefOid"].as_str().map(CommitId::from).ok_or_else(incomplete)?;
        headed.push(Headed { number, state, head });
    }
    Ok(headed)
}
