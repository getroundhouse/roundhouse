use protocol::client::ErrorCode;
use protocol::model::{
    ChecksRollup, GithubRepositoryName, IssueNumber, IssueState, Reference, ReferenceKind, ReferenceLabel,
    ReferencedPullRequest, Review,
};
use serde_json::{Value, json};

mod parsing;
mod query;
mod reading;
