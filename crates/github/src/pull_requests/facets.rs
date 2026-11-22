//! What a read finds of a pull request, in facets that can each be kept with the time it was read, and how a pull
//! request is put together from them, as the bar and waits read it.

use protocol::client::ApiError;
use protocol::model::{
    Checks as JudgedChecks, CommitId, GithubRepositoryName, History, LeftQueue, PrState, PullRequest,
    PullRequestNumber, Read, Stack, Summary,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::parse::{Metadata, Policy, ReportedCheck, time};
use super::said::{array, changed, incomplete, string};

/// The checks reported for the commit GitHub gives a pull request's checks on, which is its head, or the merge queue's
/// test merge of it, as they're kept before its base branch's rules say which are required.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checks {
    /// The pull request's head as they were read, which they're of, so a newer head's checks aren't taken for them.
    pub head: CommitId,
    /// The commit they ran on, or `None` when GitHub has none for it.
    pub revision: Option<CommitId>,
    pub reported: Vec<ReportedCheck>,
}

/// A branch's rules for the checks a pull request into it needs.
pub type Rules = Policy;

/// What `pr`, as GitHub answered for its fields, says of when changes were last requested and it last left the merge
/// queue without merging.
pub(super) fn history(pr: &Value) -> Result<History, ApiError> {
    let last = array(&pr["reviews"], "nodes")?.last();
    let left_queue = match array(&pr["timelineItems"], "nodes")?.last() {
        Some(event) if event["reason"] != "merged" => {
            Some(LeftQueue { at_ms: time(&event["createdAt"])?, reason: string(event, "reason")? })
        }
        Some(_) | None => None,
    };
    Ok(History { changes_requested_at_ms: last.map(|review| time(&review["submittedAt"])).transpose()?, left_queue })
}

/// Where `metadata`, read whole, puts it, and whether it's linear, as `pr`, its fields read again with how far its
/// head is behind the one under it, says.
pub(super) fn stack(pr: &Value, metadata: &Metadata) -> Result<Stack, ApiError> {
    Ok(Stack {
        destination: metadata.destination.as_str().into(),
        dependencies: metadata.dependencies.clone(),
        linear: linear(pr, metadata)?,
    })
}

/// Whether the one under it is an ancestor of its head, as `pr`'s comparison of the two says, which has to be of the
/// commits it was read at.
fn linear(pr: &Value, metadata: &Metadata) -> Result<Option<bool>, ApiError> {
    if metadata.dependencies.is_empty() {
        return Ok(Some(true));
    }
    let compare = &pr["baseRef"]["compare"];
    if compare.is_null() {
        return Ok(None);
    }
    if string(&compare["baseTarget"], "oid")? != metadata.base.as_str()
        || string(&compare["headTarget"], "oid")? != metadata.head.as_str()
    {
        return Err(changed());
    }
    Ok(Some(compare["behindBy"].as_u64().ok_or_else(incomplete)? == 0))
}

/// The checks `checks` reported, as `rules`, its destination's, say which are required, or none for one that's merged
/// or closed, whose checks no longer matter. Without rules, an open one's requirements aren't known.
pub fn judged(checks: Checks, summary: &Summary, rules: Option<&Rules>) -> JudgedChecks {
    let done = matches!(summary.state, PrState::Merged | PrState::Closed);
    let empty = Rules::default();
    let observed = if done { Vec::new() } else { rules.unwrap_or(&empty).checks(checks.reported) };
    JudgedChecks { revision: checks.revision, requirements_known: done || rules.is_some(), checks: observed }
}

/// The pull request `number` of `repository` its `facets` make, every facet read at `read_at_ms`, as the bar and
/// waits read it, with the checks `rules`, its destination's, say are required.
pub fn observe(
    repository: GithubRepositoryName,
    number: PullRequestNumber,
    facets: PullFacets,
    rules: Option<&Rules>,
    read_at_ms: i64,
) -> PullRequest {
    let PullFacets { summary, history, stack, checks } = facets;
    let checks = judged(checks, &summary, rules);
    let read = |said| Read { said, read_at_ms };
    PullRequest {
        repository,
        number,
        summary: read(summary),
        history: Some(Read { said: history, read_at_ms }),
        stack: Some(Read { said: stack, read_at_ms }),
        checks: Some(Read { said: checks, read_at_ms }),
    }
}

/// What a read found of one pull request, in facets, each read at the same moment, which [`observe`] puts together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullFacets {
    pub summary: Summary,
    pub history: History,
    pub stack: Stack,
    pub checks: Checks,
}
