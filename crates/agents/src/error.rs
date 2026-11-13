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
    /// The agent doesn't know the request, such as one a newer CLI added, so asking again is pointless. This is its
    /// own wording.
    #[error("{0}")]
    UnknownRequest(String),
    /// A level of effort this agent has no name for, such as one a newer build chose. The way out is to update, or to
    /// choose another level, since running at one nobody chose would be wrong.
    #[error(
        "this build has no name for that effort, so the agent can't use it. Update Roundhouse, or choose another level"
    )]
    UnknownEffort,
    /// A chat can't be given [`protocol::model::PermissionMode::Unknown`], and the hub refuses it before it gets here.
    #[error("this build has no name for that approval mode, so the agent can't use it")]
    UnknownMode,
    /// The agent said yes to a level of effort, then reported using `applied` instead, or said nothing of one.
    #[error("the agent kept its effort at {}, rather than {asked}", applied.as_deref().unwrap_or("what it was"))]
    NotApplied { asked: &'static str, applied: Option<String> },
    #[error("couldn't write to the agent")]
    Write(#[from] io::Error),
    #[error("couldn't encode a message for the agent")]
    Encode(#[from] serde_json::Error),
    #[error("the CLI gave no title")]
    NoTitle(#[from] NoTitle),
    #[error("the CLI didn't say whose account it's logged in to")]
    NoAccount(#[from] NoAccount),
    #[error("a one-off call to the CLI failed")]
    Call(#[from] CallFailed),
}

/// Why asking the CLI whose account it's logged in to gave nothing to use.
#[derive(Debug, thiserror::Error)]
pub enum NoAccount {
    #[error(transparent)]
    Call(#[from] CallFailed),
    /// It answered, but not with whether anyone is logged in, so nothing it said can be taken as an account.
    #[error("it didn't say whether anyone is logged in")]
    Unreadable,
    /// It was given a login its hub holds, and runs under something else, which its own settings can put before a
    /// token, such as an `env` that sets an API key. So what it says isn't the account the hub chose.
    #[error("it runs under `{under}` rather than the login it was given, which its settings must put first")]
    Outranked { under: String },
}

/// Why a one-off call to the CLI came back with nothing to read.
#[derive(Debug, thiserror::Error)]
pub enum CallFailed {
    #[error("couldn't talk to it")]
    Io(#[source] io::Error),
    #[error("it didn't answer in {} s", .0.as_secs())]
    TimedOut(Duration),
}

/// Why a one-off call for a title gave none to use.
#[derive(Debug, thiserror::Error)]
pub enum NoTitle {
    #[error("couldn't make an empty directory to run it in")]
    NoRoom(#[source] io::Error),
    #[error(transparent)]
    Call(#[from] CallFailed),
    /// What it said is the `result` of its `result` line if it had one, such as that nobody is logged in, or else its
    /// stderr.
    #[error("it exited with {status}: {said}")]
    Failed { status: ExitStatus, said: String },
    /// It printed no `result` line, the one that holds the answer.
    #[error("it printed no result")]
    NoResult,
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
