//! Issues and pull requests on GitHub that a chat's text links to, as the hub looks each up for every client, so a
//! reply can draw each as a chip that says where it stands.

use serde::{Deserialize, Serialize};

use super::{GithubRepositoryName, PullRequest, Read};

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

/// An issue or a pull request a text links to, as the hub found it on GitHub.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[expect(
    clippy::large_enum_variant,
    reason = "a chip names issues as often as pull requests, each kept once, and a read sends it boxed"
)]
pub enum Reference {
    Issue(Issue),
    PullRequest(PullRequest),
    /// One this version doesn't know.
    #[serde(other)]
    Unknown,
}

/// An issue on GitHub, as the hub last read it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// The repository it's in, as GitHub names it, which can differ from how it was asked for in case.
    pub repository: GithubRepositoryName,
    pub number: IssueNumber,
    pub summary: Read<IssueSummary>,
}

/// What GitHub says of an issue.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct IssueSummary {
    pub about: About,
    pub state: IssueState,
}

/// What an issue and a pull request both say of themselves, which a chip's card shows.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct About {
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

/// A label on an issue or a pull request, in its colour on GitHub.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ReferenceLabel {
    pub name: String,
    /// As GitHub keeps it, six hex digits, such as `d876e3`.
    pub color: String,
}

impl Reference {
    /// Its repository and number, as an ask for it names it, or `None` for one this version doesn't know.
    pub fn asked(&self) -> Option<ReferenceAsk> {
        match self {
            Reference::Issue(issue) => {
                Some(ReferenceAsk { repository: issue.repository.clone(), number: issue.number })
            }
            Reference::PullRequest(pull_request) => Some(ReferenceAsk {
                repository: pull_request.repository.clone(),
                number: IssueNumber::try_from(pull_request.number.get()).ok()?,
            }),
            Reference::Unknown => None,
        }
    }

    /// What it says of itself, or `None` for one this version doesn't know.
    pub fn about(&self) -> Option<&About> {
        match self {
            Reference::Issue(issue) => Some(&issue.summary.said.about),
            Reference::PullRequest(pull_request) => Some(&pull_request.said().about),
            Reference::Unknown => None,
        }
    }
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
