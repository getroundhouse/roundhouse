//! What a read finds of a pull request, in facets that can each be kept with the time it was read, and how a pull
//! request is put together from them, as the bar and waits read it.

use protocol::client::ApiError;
use protocol::model::{
    BranchName, CheckState, Checks as CheckCounts, CommitId, LeftQueue, Local, MergeState, Mergeability,
    ObservedPullRequest, PrState, PullRequest, PullRequestNumber, QueueState, Review,
};
use serde_json::Value;

use super::parse::{Metadata, Policy, ReportedCheck, review, time};
use super::said::{array, changed, incomplete, string};

/// Where a pull request stands on its own: what it is, the commits it's at, whether it can merge, and its place in
/// the merge queue. Its plain fields, which cost no more than asking for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Status {
    pub(crate) number: u64,
    pub(crate) url: String,
    pub(crate) title: String,
    pub(crate) state: PrState,
    pub(crate) is_draft: bool,
    /// The branch it merges into.
    pub(crate) base_name: BranchName,
    pub(crate) head: CommitId,
    pub(crate) base: CommitId,
    pub(crate) merge_state: MergeState,
    pub(crate) mergeable: Mergeability,
    pub(crate) review: Review,
    pub(crate) queue: Option<QueueState>,
    /// The last time it left the merge queue without merging.
    pub(crate) left_queue: Option<LeftQueue>,
}

/// What its reviews say, beyond the decision its status has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reviews {
    /// When a review last asked for changes, unless it's been dismissed.
    pub(crate) changes_requested_at_ms: Option<i64>,
}

/// Where it is in its stack, if it's in one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Stack {
    /// Where it merges in the end: its stack's trunk, or the branch it merges into when it's in none.
    pub(crate) destination: BranchName,
    /// Those open under it, lowest first.
    pub(crate) dependencies: Vec<PullRequestNumber>,
    /// Whether the one just under it is an ancestor of its head, or `None` when that couldn't be told.
    pub(crate) linear: Option<bool>,
}

/// The checks reported for the commit GitHub gives a pull request's checks on, which is its head, or the merge queue's
/// test merge of it, with whether GitHub says each is required of it.
#[derive(Debug)]
pub(crate) struct Checks {
    /// The commit they ran on, or `None` when GitHub has none for it.
    pub(crate) revision: Option<CommitId>,
    pub(crate) reported: Vec<ReportedCheck>,
}

/// A branch's rules for the checks a pull request into it needs.
pub(crate) type Rules = Policy;

impl Status {
    /// What `pr`, as GitHub answered for its fields, says.
    pub(crate) fn read(pr: &Value) -> Result<Self, ApiError> {
        let parse = |key: &str| pr.get(key).cloned().ok_or_else(incomplete);
        let entry = pr.get("mergeQueueEntry").ok_or_else(incomplete)?;
        let queue = if entry.is_null() {
            None
        } else {
            Some(serde_json::from_value(entry["state"].clone()).map_err(|_| incomplete())?)
        };
        let left_queue = match array(&pr["timelineItems"], "nodes")?.last() {
            Some(event) if event["reason"] != "merged" => {
                Some(LeftQueue { at_ms: time(&event["createdAt"])?, reason: string(event, "reason")? })
            }
            Some(_) | None => None,
        };
        Ok(Self {
            number: pr["number"].as_u64().ok_or_else(incomplete)?,
            url: string(pr, "url")?,
            title: string(pr, "title")?,
            state: serde_json::from_value(parse("state")?).map_err(|_| incomplete())?,
            is_draft: pr["isDraft"].as_bool().ok_or_else(incomplete)?,
            base_name: BranchName::from(string(pr, "baseRefName")?),
            head: string(pr, "headRefOid")?.into(),
            base: string(pr, "baseRefOid")?.into(),
            merge_state: serde_json::from_value(parse("mergeStateStatus")?).map_err(|_| incomplete())?,
            mergeable: match pr["mergeable"].as_str() {
                Some("MERGEABLE") => Mergeability::Mergeable,
                Some("CONFLICTING") => Mergeability::Conflicting,
                _ => Mergeability::Unknown,
            },
            review: review(pr)?,
            queue,
            left_queue,
        })
    }
}

impl Reviews {
    /// What `pr`, as GitHub answered for its fields, says of its reviews.
    pub(crate) fn read(pr: &Value) -> Result<Self, ApiError> {
        let last = array(&pr["reviews"], "nodes")?.last();
        Ok(Self { changes_requested_at_ms: last.map(|review| time(&review["submittedAt"])).transpose()? })
    }
}

impl Stack {
    /// Where `metadata`, read whole, puts it, and whether it's linear, as `pr`, its fields read again with how far its
    /// head is behind the one under it, says.
    pub(crate) fn read(pr: &Value, metadata: &Metadata) -> Result<Self, ApiError> {
        Ok(Self {
            destination: metadata.destination.as_str().into(),
            dependencies: metadata.dependencies.clone(),
            linear: linear(pr, metadata)?,
        })
    }
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

/// The pull request its facets make, as the bar and waits read it: `status`, `reviews` and `stack`, with `checks`,
/// which `rules`, its destination's, say which are required, or none for one that's merged or closed, whose checks no
/// longer matter.
pub(crate) fn observe(
    status: Status,
    reviews: Reviews,
    stack: Stack,
    checks: Checks,
    rules: Option<&Rules>,
) -> ObservedPullRequest {
    let done = matches!(status.state, PrState::Merged | PrState::Closed);
    let empty = Rules::default();
    let observed = if done { Vec::new() } else { rules.unwrap_or(&empty).checks(checks.reported) };
    let mut counts = CheckCounts::default();
    for check in &observed {
        match check.state {
            CheckState::Passed => counts.passed += 1,
            CheckState::Failed => counts.failed += 1,
            CheckState::Pending | CheckState::Unknown => counts.pending += 1,
        }
    }
    let pull_request = PullRequest {
        number: status.number,
        url: status.url,
        title: status.title,
        state: status.state,
        is_draft: status.is_draft,
        base: Some(status.base_name),
        merge_state: status.merge_state,
        review: status.review,
        checks: counts,
        queue: status.queue,
        left_queue: status.left_queue,
        changes_requested_at_ms: reviews.changes_requested_at_ms,
        local: Local::default(),
    };
    ObservedPullRequest {
        pull_request,
        head: status.head,
        base: status.base,
        destination: stack.destination,
        dependencies: stack.dependencies,
        mergeability: status.mergeable,
        linear: stack.linear,
        checks: observed,
        checks_known: checks.revision.is_some(),
        checks_revision: checks.revision,
        requirements_known: true,
    }
}
