//! What GitHub answered for an issue or a pull request, as the hub keeps it, or why none of it can be taken: a part of
//! it missing, or written otherwise than this reads, takes nothing.

use protocol::client::ApiError;
use protocol::model::{
    About, ChecksRollup, GithubRepositoryName, Issue, IssueNumber, IssueState, IssueSummary, Mergeability, PullRequest,
    PullRequestNumber, REFERENCE_BODY_CHARS, Read, Reference, ReferenceLabel, Summary,
};
use serde_json::Value;

use crate::pull_requests::{array, incomplete, queue, review, string, time};

/// The issue or pull request `node` is, in `repository`, as GitHub answered for them in a read that began at
/// `read_at_ms`.
pub(super) fn reference(repository: &Value, node: &Value, read_at_ms: i64) -> Result<Reference, ApiError> {
    let repository = GithubRepositoryName::try_from(string(repository, "nameWithOwner")?).map_err(|_| incomplete())?;
    match node["__typename"].as_str() {
        Some("Issue") => {
            let number = number(node).and_then(|number| IssueNumber::try_from(number).ok()).ok_or_else(incomplete)?;
            let said = IssueSummary { about: about(node)?, state: issue_state(node)? };
            Ok(Reference::Issue(Issue { repository, number, summary: Read { said, read_at_ms } }))
        }
        Some("PullRequest") => pull_request_reference(repository, node, read_at_ms),
        _ => Err(incomplete()),
    }
}

/// The pull request `node` is, in `repository`, with the summary GitHub answered for in a read that began at
/// `read_at_ms`, as its [`pull_request_summary`](super::query::pull_request_summary) asks for it.
pub(crate) fn pull_request_reference(
    repository: GithubRepositoryName,
    node: &Value,
    read_at_ms: i64,
) -> Result<Reference, ApiError> {
    let number = number(node).and_then(|number| PullRequestNumber::try_from(number).ok()).ok_or_else(incomplete)?;
    Ok(Reference::PullRequest(PullRequest {
        repository,
        number,
        summary: Read { said: summary(node)?, read_at_ms },
        history: None,
        stack: None,
        checks: None,
    }))
}

/// What a pull request says, where it stands, the commits it's at, what it changes, and where its newest commit's
/// checks stand together.
pub(crate) fn summary(node: &Value) -> Result<Summary, ApiError> {
    let newest = array(&node["commits"], "nodes")?.last();
    let rollup = newest.and_then(|newest| newest["commit"]["statusCheckRollup"]["state"].as_str());
    let parse = |key: &str| node.get(key).cloned().ok_or_else(incomplete);
    Ok(Summary {
        about: about(node)?,
        state: serde_json::from_value(parse("state")?).map_err(|_| incomplete())?,
        is_draft: node["isDraft"].as_bool().ok_or_else(incomplete)?,
        review: review(node)?,
        queue: queue(node)?,
        merge_state: serde_json::from_value(parse("mergeStateStatus")?).map_err(|_| incomplete())?,
        mergeable: match node["mergeable"].as_str() {
            Some("MERGEABLE") => Mergeability::Mergeable,
            Some("CONFLICTING") => Mergeability::Conflicting,
            _ => Mergeability::Unknown,
        },
        base_branch: string(node, "baseRefName")?.into(),
        head: string(node, "headRefOid")?.as_str().into(),
        base: string(node, "baseRefOid")?.as_str().into(),
        added: count(&node["additions"])?,
        removed: count(&node["deletions"])?,
        rollup: rollup.map(checks),
    })
}

/// What an issue or a pull request says of itself.
fn about(node: &Value) -> Result<About, ApiError> {
    Ok(About {
        url: string(node, "url")?,
        title: string(node, "title")?,
        author: node["author"]["login"].as_str().map(str::to_owned),
        opened_at_ms: time(&node["createdAt"])?,
        comments: count(&node["comments"]["totalCount"])?,
        labels: array(&node["labels"], "nodes")?.iter().map(label).collect::<Result<_, _>>()?,
        body: string(node, "bodyText")?.chars().take(REFERENCE_BODY_CHARS).collect(),
    })
}

fn number(node: &Value) -> Option<u64> {
    node["number"].as_u64()
}

/// Where an issue stands: closed as done, which is how GitHub closes one unless it's told why, or as not planned,
/// which a duplicate is too.
fn issue_state(node: &Value) -> Result<IssueState, ApiError> {
    Ok(match string(node, "state")?.as_str() {
        "OPEN" => IssueState::Open,
        "CLOSED" => match node["stateReason"].as_str() {
            Some("COMPLETED") | None => IssueState::Done,
            Some("NOT_PLANNED" | "DUPLICATE") => IssueState::NotPlanned,
            Some(_) => IssueState::Unknown,
        },
        _ => IssueState::Unknown,
    })
}

/// GitHub's `StatusState` for a commit's checks together.
fn checks(state: &str) -> ChecksRollup {
    match state {
        "SUCCESS" => ChecksRollup::Passed,
        "FAILURE" | "ERROR" => ChecksRollup::Failed,
        "PENDING" | "EXPECTED" => ChecksRollup::Running,
        _ => ChecksRollup::Unknown,
    }
}

fn label(label: &Value) -> Result<ReferenceLabel, ApiError> {
    Ok(ReferenceLabel { name: string(label, "name")?, color: string(label, "color")? })
}

fn count(value: &Value) -> Result<u32, ApiError> {
    value.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(incomplete)
}
