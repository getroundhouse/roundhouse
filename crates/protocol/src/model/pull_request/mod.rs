//! A pull request on GitHub, as the hub reads it in facets, what it is and where a workspace's work stands against
//! it: what's in the way of merging it, most pressing first.

#[cfg(test)]
mod tests;

mod facets;
mod link;
mod phase;
mod standing;
mod status;
mod unseen;
pub use facets::{
    CheckCounts, CheckState, Checks, ChecksRollup, History, Mergeability, ObservedCheck, PullRequest, Read, Stack,
    Summary,
};
pub use link::{PullRequestLink, PullRequestLinked};
pub use phase::Phase;
pub use standing::Standing;
pub(crate) use status::deserialize_looked;
pub use status::{LeftQueue, Local, MergeState, OwnPullRequest, PrState, PullRequestFound, QueueState, Review};
pub use unseen::RepositoryUnseen;
