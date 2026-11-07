use std::io;

use protocol::client::{ApiError, ErrorCode, error_chain};

use crate::ENV_CLAUDE_BIN;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("could not find the `claude` CLI. Install it, or set {ENV_CLAUDE_BIN} to its path")]
    NotInstalled,
    #[error("could not start {bin} in {cwd}")]
    Start { bin: String, cwd: String, source: io::Error },
    #[error("{bin} started without its pipes")]
    NoPipes { bin: String },
    #[error("the agent has exited")]
    Exited,
    #[error("the agent didn't answer")]
    NoAnswer,
    /// The agent understood the request and said no. This is its own wording.
    #[error("{0}")]
    Refused(String),
    #[error("couldn't write to the agent")]
    Write(#[from] io::Error),
    #[error("couldn't encode a message for the agent")]
    Encode(#[from] serde_json::Error),
}

impl From<AgentError> for ApiError {
    fn from(error: AgentError) -> Self {
        ApiError::new(ErrorCode::Worker, error_chain(&error))
    }
}
