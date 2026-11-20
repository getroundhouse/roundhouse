//! Issues and pull requests on GitHub that a chat's text links to, as the hub looks each up for every client, so a
//! reply can draw each as a chip that says where it stands.

use serde::{Deserialize, Serialize};

use super::{GithubRepositoryName, Review};

/// An issue's or a pull request's number in its repository, which GitHub counts together, from 1 to GraphQL's largest
/// whole number.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(try_from = "u64")]
pub struct IssueNumber(u64);

/// Why a number names no issue.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("an issue's or a pull request's number is between 1 and 2147483647")]
pub struct IssueNumberError;

impl IssueNumber {
    pub fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for IssueNumber {
    type Error = IssueNumberError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        let largest = u64::try_from(i32::MAX).map_err(|_| IssueNumberError)?;
        if value == 0 || value > largest {
            return Err(IssueNumberError);
        }
        Ok(Self(value))
    }
}

impl std::fmt::Display for IssueNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// An issue or a pull request as a text links to it on GitHub: its repository and its number.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReferenceAsk {
    pub repository: GithubRepositoryName,
    pub number: IssueNumber,
}

/// What the hub found of an issue or a pull request on GitHub.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    /// Where it is, as GitHub names it, which can differ from how it was asked for in case.
    pub repository: GithubRepositoryName,
    pub number: IssueNumber,
    /// Whether it's an issue or a pull request, and where it stands.
    pub kind: ReferenceKind,
    pub url: String,
    pub title: String,
    /// Who opened it, by their login, or `None` for an account that's gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub opened_at_ms: i64,
    pub comments: u32,
    #[serde(default)]
    pub labels: Vec<ReferenceLabel>,
    /// The start of what it says, as plain text, cut short, as [`REFERENCE_BODY_CHARS`] says.
    #[serde(default)]
    pub body: String,
}

/// How much of what an issue or a pull request says the hub keeps, in characters, which is more than a card shows,
/// so each client can cut it where it fits.
pub const REFERENCE_BODY_CHARS: usize = 400;

/// Whether it's an issue or a pull request, with where it stands, which each has its own states for.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReferenceKind {
    Issue {
        state: IssueState,
    },
    PullRequest(ReferencedPullRequest),
    /// One this version doesn't know.
    #[serde(other)]
    Unknown,
}

/// Where an issue stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IssueState {
    Open,
    /// Closed as done.
    Done,
    /// Closed as not planned, or as a duplicate.
    NotPlanned,
    /// One this version doesn't know.
    #[serde(other)]
    Unknown,
}

/// Where a pull request stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestState {
    Open,
    /// Open, as a draft.
    Draft,
    Merged,
    /// Closed without merging.
    Closed,
    /// One this version doesn't know.
    #[serde(other)]
    Unknown,
}

/// A label on an issue or a pull request, in its colour on GitHub.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ReferenceLabel {
    pub name: String,
    /// As GitHub keeps it, six hex digits, such as `d876e3`.
    pub color: String,
}

/// Where a pull request a text links to stands, and what it changes.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferencedPullRequest {
    pub state: PullRequestState,
    pub added: u32,
    pub removed: u32,
    /// Where its checks stand on its newest commit, or `None` when it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<ChecksRollup>,
    #[serde(default)]
    pub review: Review,
}

/// GitHub's word for where a commit's checks stand together.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChecksRollup {
    Passed,
    Failed,
    /// Queued, running, or waiting to be reported.
    Running,
    /// One this version doesn't know.
    #[serde(other)]
    Unknown,
}

/// What the hub could say of an issue or a pull request that was asked for.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "found", rename_all = "snake_case")]
pub enum ReferenceRead {
    /// What it found, or last found, if it can't be read now.
    Found { reference: Box<Reference> },
    /// GitHub says there's no such issue or pull request that the hub can see, as in a private repository its token
    /// can't read, so it's left as the text named it.
    Missing,
    /// It couldn't be read, and nothing was known of it, with why, which may pass, such as while GitHub can't be
    /// reached.
    Unread { message: String },
    /// One this version doesn't know.
    #[serde(other)]
    Unknown,
}
