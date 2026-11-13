//! The pull request of the branch a worktree has checked out, from `gh`, and how the worktree stands against it.

use self::local::local;

mod link;
mod local;
#[cfg(test)]
mod tests;

mod github;

#[cfg(test)]
use github::{ByUrl, Failing, Viewed, failing, parse_by_url, parse_commits, parse_pull_request, parse_rate_limit};
pub use github::{pull_request, rate_limit};
