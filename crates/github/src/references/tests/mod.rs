use protocol::client::ErrorCode;
use protocol::model::{
    About, ChecksRollup, GithubRepositoryName, Issue, IssueNumber, IssueState, IssueSummary, PullRequest, Read,
    Reference, ReferenceLabel, Review, Summary,
};
use serde_json::{Value, json};

mod parsing;
mod query;
mod reading;
