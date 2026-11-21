use super::said::RateLimit;
use protocol::client::ApiError;
use protocol::model::{GithubRepositoryName, PullRequestNumber, PullRequestSelection, PullRequestsSnapshot, Reference};

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
    /// The summary of each pull request the read looked at, as a chip shows it, or why it couldn't be read, whether
    /// the rest of its ask could be or not.
    pub summaries: Vec<SummaryRead>,
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

/// What a read found of one pull request's summary.
#[derive(Debug)]
pub struct SummaryRead {
    /// The repository it was asked of, as it was asked.
    pub repository: GithubRepositoryName,
    pub number: PullRequestNumber,
    pub summary: Result<Reference, ApiError>,
}
