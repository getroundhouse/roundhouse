//! Reading pull requests from GitHub's GraphQL API, as the hub does for the pull requests agents ask for and wait on:
//! each with its checks and which of them are required, and those open under it in its stack, many at once, as one
//! token, in as few queries as GitHub allows.

mod asked;
pub(crate) use asked::{PullRequestsAsk, Unread};
mod graphql;
mod heads;
pub(crate) use heads::{Headed, HeadsAsk};
mod parse;
mod query;
mod reading;
mod rounds;
pub(crate) use rounds::{Leaving, first_queries};
mod said;
#[cfg(test)]
mod tests;
