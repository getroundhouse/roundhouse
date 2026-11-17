//! Asking GitHub's GraphQL API, as a token.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::GithubToken;
use serde_json::{Value, json};

use super::said::{incomplete, token_refused};
use crate::github::Github;
use crate::github::client::within;

impl Github {
    /// What GitHub answers `query` with, as `token`, within [`ASK_WITHIN`](crate::github::ASK_WITHIN): its data and
    /// any errors, which GitHub gives beside the data it could read, so whoever asked can take what's there, or why
    /// there's none to read.
    pub(super) async fn graphql(&self, token: &GithubToken, query: &str) -> Result<Value, NoAnswer> {
        let failed = |message: String| NoAnswer::Failed(ApiError::new(ErrorCode::Worker, message));
        let body = json!({ "query": query });
        let answer = within(self.ask(http::Method::POST, "/graphql", Some(token.as_str()), Some(&body)))
            .await
            .map_err(|e| failed(e.message))?;
        if answer.limited {
            return Err(failed("GitHub is limiting how often the hub reads it, so it reads again later".to_string()));
        }
        if answer.status == 401 {
            return Err(NoAnswer::Refused);
        }
        let read: Option<Value> = serde_json::from_str(&answer.body).ok();
        match read {
            Some(read) if answer.succeeded() || read.get("errors").is_some() => Ok(read),
            _ if answer.succeeded() => Err(NoAnswer::Failed(incomplete())),
            _ => Err(failed(format!("GitHub answered {}", answer.status))),
        }
    }
}

/// Why GitHub gave a query no answer to read.
pub(super) enum NoAnswer {
    /// It refused the token, which the read is then said to have had refused, so it's not used again.
    Refused,
    /// It couldn't be reached, didn't answer in time, limited the hub, or answered with what isn't GraphQL's, each
    /// [`ErrorCode::Worker`], as what may pass.
    Failed(ApiError),
}

impl NoAnswer {
    pub(super) fn error(&self) -> ApiError {
        match self {
            Self::Refused => token_refused(),
            Self::Failed(error) => error.clone(),
        }
    }
}
