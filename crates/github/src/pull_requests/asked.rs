use super::said::RateLimit;
use protocol::client::ApiError;
use protocol::model::{GithubRepositoryName, PullRequestNumber, PullRequestSelection, PullRequestsSnapshot};

/// What one ask of a read is for: the pull requests `selection` names in `repository`, and those open under them in
/// their stacks on GitHub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestsAsk {
    pub repository: GithubRepositoryName,
    pub selection: PullRequestSelection,
}

/// What a read found for each of its asks, in the order they were asked.
#[derive(Debug)]
pub struct PullRequestsRead {
    pub answers: Vec<Result<PullRequestsSnapshot, Unread>>,
    /// What GitHub last said is left of its limit for the token, if it said.
    pub rate_limit: Option<RateLimit>,
    /// Whether GitHub refused the token, so whoever holds it can find out whether it's still the one to read with.
    pub refused: bool,
}

/// Why an ask couldn't be read, and which of its pull requests that's about: the one GitHub didn't answer for and
/// those above it in its stack, or none when it's about the whole ask, such as its repository.
#[derive(Clone, Debug, PartialEq)]
pub struct Unread {
    pub error: ApiError,
    pub affected: Vec<PullRequestNumber>,
}
