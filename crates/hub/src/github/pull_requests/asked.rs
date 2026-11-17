use protocol::client::ApiError;
use protocol::model::{GithubRepositoryName, PullRequestNumber, PullRequestSelection, PullRequestsSnapshot};
use protocol::worker::RateLimit;

/// What one ask of a read is for: the pull requests `selection` names in `repository`, and those open under them in
/// their stacks on GitHub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PullRequestsAsk {
    pub(crate) repository: GithubRepositoryName,
    pub(crate) selection: PullRequestSelection,
}

/// What a read found for each of its asks, in the order they were asked, with what it cost.
#[derive(Debug)]
pub(crate) struct PullRequestsRead {
    pub(crate) answers: Vec<Result<PullRequestsSnapshot, Unread>>,
    /// What GitHub last said is left of its limit for the token, if it said.
    pub(crate) rate_limit: Option<RateLimit>,
    /// The points the read spent, as GitHub counted each query, and one for each it didn't say of.
    pub(crate) cost: u32,
    /// Whether GitHub refused the token, so whoever holds it can find out whether it's still the one to read with.
    pub(crate) refused: bool,
}

/// Why an ask couldn't be read, and which of its pull requests that's about: the one GitHub didn't answer for and
/// those above it in its stack, or none when it's about the whole ask, such as its repository.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Unread {
    pub(crate) error: ApiError,
    pub(crate) affected: Vec<PullRequestNumber>,
}
