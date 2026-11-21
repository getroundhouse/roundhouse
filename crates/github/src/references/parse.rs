//! What GitHub answered for an issue or a pull request, as the hub keeps it, or why none of it can be taken: a part of
//! it missing, or written otherwise than this reads, takes nothing.

use protocol::client::ApiError;
use protocol::model::{
    ChecksRollup, GithubRepositoryName, IssueNumber, IssueState, PullRequestState, REFERENCE_BODY_CHARS, Reference,
    ReferenceKind, ReferenceLabel, ReferencedPullRequest,
};
use serde_json::Value;

use crate::pull_requests::{array, incomplete, review, string, time};

/// The issue or pull request `node` is, in `repository`, as GitHub answered for them.
pub(super) fn reference(repository: &Value, node: &Value) -> Result<Reference, ApiError> {
    let kind = match node["__typename"].as_str() {
        Some("Issue") => ReferenceKind::Issue { state: issue_state(node)? },
        Some("PullRequest") => ReferenceKind::PullRequest(pull_request(node)?),
        _ => return Err(incomplete()),
    };
    let repository = GithubRepositoryName::try_from(string(repository, "nameWithOwner")?).map_err(|_| incomplete())?;
    reference_of(repository, kind, node)
}

/// The summary of the pull request `node` is, in `repository`, as GitHub answered for its
/// [`pull_request_summary`](super::query::pull_request_summary).
pub(crate) fn pull_request_reference(repository: GithubRepositoryName, node: &Value) -> Result<Reference, ApiError> {
    reference_of(repository, ReferenceKind::PullRequest(pull_request(node)?), node)
}

/// The issue or pull request `node` is, in `repository`, which is `kind`.
fn reference_of(repository: GithubRepositoryName, kind: ReferenceKind, node: &Value) -> Result<Reference, ApiError> {
    let labels = array(&node["labels"], "nodes")?.iter().map(label).collect::<Result<_, _>>()?;
    Ok(Reference {
        repository,
        number: node["number"].as_u64().and_then(|n| IssueNumber::try_from(n).ok()).ok_or_else(incomplete)?,
        kind,
        url: string(node, "url")?,
        title: string(node, "title")?,
        author: node["author"]["login"].as_str().map(str::to_owned),
        opened_at_ms: time(&node["createdAt"])?,
        comments: count(&node["comments"]["totalCount"])?,
        labels,
        body: string(node, "bodyText")?.chars().take(REFERENCE_BODY_CHARS).collect(),
    })
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

/// Where a pull request stands: open, as a draft or not, merged, or closed without merging.
fn pull_request_state(node: &Value) -> Result<PullRequestState, ApiError> {
    Ok(match string(node, "state")?.as_str() {
        "OPEN" if node["isDraft"].as_bool().ok_or_else(incomplete)? => PullRequestState::Draft,
        "OPEN" => PullRequestState::Open,
        "MERGED" => PullRequestState::Merged,
        "CLOSED" => PullRequestState::Closed,
        _ => PullRequestState::Unknown,
    })
}

/// Where a pull request stands, what it changes, and where its checks, on its newest commit, and its review stand.
fn pull_request(node: &Value) -> Result<ReferencedPullRequest, ApiError> {
    let newest = array(&node["commits"], "nodes")?.last();
    let rollup = newest.and_then(|newest| newest["commit"]["statusCheckRollup"]["state"].as_str());
    Ok(ReferencedPullRequest {
        state: pull_request_state(node)?,
        added: count(&node["additions"])?,
        removed: count(&node["deletions"])?,
        checks: rollup.map(checks),
        review: review(node)?,
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
