//! A workspace's pull request, as GitHub reports it, and where its branch stands: what's in the way of merging it,
//! most pressing first.

#[cfg(test)]
mod tests;

mod link;
mod phase;
mod status;
pub use link::{PullRequestLink, PullRequestLinked};
pub use phase::Phase;
pub(crate) use status::deserialize_looked;
pub use status::{
    Checks, LeftQueue, Local, MergeState, PrState, PullRequest, PullRequestFound, QueueState, Review, Standing,
};
