//! `get_pull_request`: where the workspace's pull request stands, as the hub reads it from GitHub and works it out.
//! It's for the agent, which reads it as words, so it says in full what the app's pull request bar says in a few, and
//! says the things an agent gets wrong, such as a queued pull request not being merged yet.

#[cfg(test)]
use protocol::model::{Checks, PullRequest, QueueState, Review};

#[cfg(test)]
pub(super) mod tests;

mod description;
pub(super) use description::{NAME, call, describe, standing};
