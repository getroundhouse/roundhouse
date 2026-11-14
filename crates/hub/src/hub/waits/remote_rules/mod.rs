//! Aggregate waits: one blocker ends the task; success requires every watched PR on its observed revision.

use protocol::model::{
    CheckState, MergeState, Mergeability, ObservedPullRequest, PrState, PullRequestNumber, PullRequestSelection,
    PullRequestWatch, PullRequestsSnapshot, QueueState, Wait, WaitBaseline, WaitEnd, WaitEnded, WaitUntil,
};
use std::collections::BTreeSet;
#[cfg(test)]
mod tests;

mod outcomes;
pub(super) use outcomes::baseline;
pub(super) use outcomes::expire;
pub(super) use outcomes::judge;
pub(super) use outcomes::observe;
