//! A workspace's pull request, as GitHub reports it, and where its branch stands: what's in the way of merging it,
//! most pressing first.

#[cfg(test)]
mod tests;

mod status;
pub(super) use status::deserialize_found;
pub use status::{
    Checks, LeftQueue, Local, MergeState, PrState, PullRequest, PullRequestFound, QueueState, Review, Standing,
};
