use std::io;
use std::process::ExitStatus;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode, error_chain};

use crate::ENV_CLAUDE_BIN;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("this worker doesn't know that kind of agent")]
    UnknownKind,
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
    #[error("the CLI gave no title")]
    NoTitle(#[from] NoTitle),
}

/// Why a one-off call for a title gave none to use.
#[derive(Debug, thiserror::Error)]
pub enum NoTitle {
    #[error("couldn't make an empty directory to run it in")]
    NoRoom(#[source] io::Error),
    #[error("couldn't talk to it")]
    Io(#[source] io::Error),
    #[error("it didn't answer in {} s", .0.as_secs())]
    TimedOut(Duration),
    /// What it said is the `result` of its JSON if it had one, such as that nobody is logged in, or else its stderr.
    #[error("it exited with {status}: {said}")]
    Failed { status: ExitStatus, said: String },
    #[error("its answer wasn't JSON")]
    NotJson(#[source] serde_json::Error),
    /// It answered, but as an error, of this kind.
    #[error("it reported {0}")]
    Reported(String),
    #[error("it answered with nothing")]
    Empty,
}

impl From<AgentError> for ApiError {
    fn from(error: AgentError) -> Self {
        ApiError::new(ErrorCode::Worker, error_chain(&error))
    }
}
