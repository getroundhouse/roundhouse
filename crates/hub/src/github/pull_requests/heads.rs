//! Finding the pull requests whose head is a branch on GitHub, as a workspace's are: the pull requests of the branch
//! its worktree pushes to, which its upstream names. A repository and a branch name are unique on GitHub, where local
//! names repeat across machines, so the branch on GitHub is what says which pull requests a workspace's work goes to.
//! Many branches go in one query, each under aliases of its own, as the waits' reads do.
//!
//! Each branch is asked after twice in the query. By the branch itself, in the repository it's pushed to, whose
//! pull requests are its own alone, which is the answer while the branch is there. And by its name, in the
//! repository its pull requests merge into, for once it's gone, as GitHub can delete a branch as its pull request
//! merges, when the pull requests that had it as their head are found only by its name. That lists those from every
//! fork's branch of the same name too, so a name many forks use can crowd the branch's own out of the newest, but
//! only for a branch that's gone. A query costs GitHub's points by how many lists it asks for, so both lists of fifty
//! branches cost a point or so, which is less than a second query for the branches that are gone.

use protocol::client::ApiError;
use protocol::model::{BranchName, CommitId, GithubRepositoryName, GithubToken, PrState, PullRequestNumber};
use protocol::worker::RateLimit;
use serde_json::Value;

use super::graphql::NoAnswer;
use super::query::quote;
use super::rounds::{Leaving, Spent};
use super::said::{failed, incomplete};
use crate::github::Github;

/// The branches one query asks after at most. Each asks for two lists of no more than [`NEWEST`] pull requests, with
/// only what tells them apart, so a query of these costs a point or two.
const ASKS_A_QUERY: usize = 50;

/// The pull requests asked for of each branch, the newest first. One merged or closed and then another opened from
/// the same branch, or a branch name used again once its pull request has merged and the branch is gone, gives a
/// branch more than one, but one with more than this many leaves the oldest out.
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

/// The query asking after `batch`, whose asks are numbered from `first`: `r` and the number, by the branch itself, and
/// `n` and the number, by its name.
fn query(batch: &[HeadsAsk], first: usize) -> String {
    let fields = "nodes { number state headRefOid headRepository { nameWithOwner } baseRepository { nameWithOwner } }";
    let newest = format!("first:{NEWEST},orderBy:{{field:CREATED_AT,direction:DESC}}");
    let asked: Vec<String> = batch
        .iter()
        .zip(first..)
        .map(|(ask, at)| {
            let branch = quote(&format!("refs/heads/{}", ask.branch.as_str()));
            format!(
                "r{at}: repository({}) {{ h: ref(qualifiedName:{branch}) {{ associatedPullRequests({newest}) {{ \
                 {fields} }} }} }} n{at}: repository({}) {{ h: pullRequests(headRefName:{},{newest}) {{ {fields} }} }}",
                named(&ask.from),
                named(&ask.repository),
                quote(ask.branch.as_str()),
            )
        })
        .collect();
    format!("query {{ {} rateLimit {{ limit remaining resetAt }} }}", asked.join(" "))
}

/// `repository`'s owner and name, as GraphQL's `repository` takes them.
fn named(repository: &GithubRepositoryName) -> String {
    let name = repository.as_str().split_once('/').map_or("", |(_, name)| name);
    format!("owner:{},name:{}", quote(repository.owner()), quote(name))
}

/// Each ask's answer from `answer`, by its aliases, with the errors GitHub gave about either. One whose path names no
/// ask, as one about the query itself, fails them all.
fn taken(answer: &Value, batch: &[HeadsAsk], first: usize) -> Vec<Result<Vec<Headed>, ApiError>> {
    let mut errors: Vec<Option<ApiError>> = batch.iter().map(|_| None).collect();
    for error in answer["errors"].as_array().into_iter().flatten() {
        let alias = error["path"].as_array().and_then(|path| path.first()).and_then(Value::as_str);
        let at = alias.and_then(|alias| alias.strip_prefix(['r', 'n'])).and_then(|at| at.parse::<usize>().ok());
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
            None => of_branch(&answer["data"][format!("r{at}")]["h"], &answer["data"][format!("n{at}")]["h"], ask),
        })
        .collect()
}

/// The pull requests of `ask`'s branch: those `branch`, the branch itself, lists into the repository asked of, while
/// it's there, or else those `by_name` lists from the repository it's pushed to.
fn of_branch(branch: &Value, by_name: &Value, ask: &HeadsAsk) -> Result<Vec<Headed>, ApiError> {
    if branch.is_object() {
        return headed(&branch["associatedPullRequests"], "baseRepository", &ask.repository);
    }
    headed(by_name, "headRepository", &ask.from)
}

/// The pull requests in `found` whose repository `side` says is `of`, newest first. One whose head repository is gone,
/// as a fork deleted since, has no head to be the branch asked after.
fn headed(found: &Value, side: &str, of: &GithubRepositoryName) -> Result<Vec<Headed>, ApiError> {
    let nodes = found["nodes"].as_array().ok_or_else(incomplete)?;
    let mut headed = Vec::new();
    for node in nodes {
        let theirs = node[side]["nameWithOwner"].as_str();
        if !theirs.is_some_and(|theirs| theirs.eq_ignore_ascii_case(of.as_str())) {
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
