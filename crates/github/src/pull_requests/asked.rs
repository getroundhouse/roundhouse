use super::facets::{PullFacets, Rules};
use super::said::RateLimit;
use protocol::client::ApiError;
use protocol::model::{
    BranchName, GithubRepositoryName, PullRequestNumber, PullRequestSelection, PullRequestsSnapshot, Reference,
};

/// What one ask of a read is for: the pull requests `selection` names in `repository`, and those open under them in
/// their stacks on GitHub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestsAsk {
    pub repository: GithubRepositoryName,
    pub selection: PullRequestSelection,
    /// The rules of branches of `repository` the asker knows well enough, which aren't read again.
    pub known_rules: Vec<(BranchName, Rules)>,
}

/// What a read found of a branch's rules for the checks a pull request into it needs, which it read since no ask knew
/// them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RulesRead {
    /// The repository it was asked of, as it was asked.
    pub repository: GithubRepositoryName,
    pub branch: BranchName,
    pub rules: Rules,
}

/// What a read found for each of its asks, in the order they were asked.
#[derive(Debug)]
pub struct PullRequestsRead {
    pub answers: Vec<Result<PullRequestsSnapshot, Unread>>,
    /// The summary of each pull request the read looked at, as a chip shows it, or why it couldn't be read, whether
    /// the rest of its ask could be or not.
    pub summaries: Vec<SummaryRead>,
    /// The rules of each branch the read read, which a later ask can give as known.
    pub rules: Vec<RulesRead>,
    /// The facets of each pull request of each ask that was read whole, all of an ask's at the same moment.
    pub facets: Vec<FacetsRead>,
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

/// What a read found of one pull request, in facets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacetsRead {
    /// The repository it was asked of, as it was asked.
    pub repository: GithubRepositoryName,
    pub facets: PullFacets,
    /// When the read of them began, by the wall clock, since none of what it found is older.
    pub read_at_ms: i64,
}
