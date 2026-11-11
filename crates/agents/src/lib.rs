//! Agent adapters. Each one runs an agent as a child process and turns its output into normalized
//! events, while keeping every raw line so those events can be rebuilt later.

#![warn(clippy::wildcard_enum_match_arm)]

pub mod claude;
mod error;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use protocol::attachment::{AttachmentFile, NewAttachment};
use protocol::event::{ApprovalDecision, EventBody};
use protocol::model::{
    AgentAccount, AgentKind, ApprovalId, ContextUsage, Seq, SessionSettingsChange, Title, UsageLimit,
};
use protocol::tail::StreamDelta;
use protocol::worker::AgentSpec;
use tokio::sync::mpsc;

pub use error::{AgentError, CallFailed, NoAccount, NoTitle};

#[derive(Debug)]
pub enum AgentOutput {
    Event {
        body: EventBody,
        raw: Option<String>,
        /// Images the event names, such as one a tool sent back, for the hub to keep. `raw` is without their bytes.
        attachments: Vec<NewAttachment>,
    },
    Delta(StreamDelta),
    /// How full the agent's context is now. It answers the adapter's own question rather than being something the
    /// agent said, and only the newest matters, so it isn't an event.
    Context(ContextUsage),
    /// What's used of each of the account's limits now, shortest window first, when the adapter asked rather than
    /// the agent saying so by itself. It answers the adapter's own question, so it isn't an event, as `Context` isn't.
    Limits(Vec<UsageLimit>),
    /// Always the last thing sent.
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
        stderr_tail: Vec<String>,
        requested: bool,
    },
}

impl AgentOutput {
    /// An event that names no images.
    pub fn event(body: EventBody, raw: Option<String>) -> Self {
        AgentOutput::Event { body, raw, attachments: Vec::new() }
    }
}

/// Names the Claude CLI directly, for tests and unusual installs.
pub use protocol::paths::ENV_CLAUDE_BIN;

/// How many milliseconds to wait for an agent's output once it has exited, in place of the usual few
/// seconds, so a test can do what it needs before that wait ends.
#[cfg(feature = "testing")]
pub const ENV_DRAIN_GRACE_MS: &str = "ROUNDHOUSE_DRAIN_GRACE_MS";

/// Where to look for an agent's CLI. `main` reads these from the environment.
#[derive(Clone, Debug, Default)]
pub struct BinaryLookup {
    /// Used as given, without looking anywhere else.
    pub explicit: Option<PathBuf>,
    /// The `PATH` to search, as the OS holds it.
    pub path: Option<OsString>,
    /// For the places the installers use under it.
    pub home: Option<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub struct AgentOptions {
    pub binary: BinaryLookup,
    /// Records the running process, so something can stop it if this worker is killed. See
    /// `providers::local`. Removed when the process exits.
    pub pid_file: Option<PathBuf>,
}

#[async_trait]
pub trait AgentProcess: Send + Sync {
    fn pid(&self) -> u32;
    /// The process has exited, or can't be reached. A new prompt needs a new process, even though
    /// its last output may still be on the way.
    fn is_gone(&self) -> bool;
    /// An error means this process can't take the prompt, and [`AgentProcess::is_gone`] is now true. `text` can be
    /// empty when there are attachments, which are in the worktree already. `prompt` is where the prompt is recorded,
    /// which the process names when it reports taking or dropping it.
    async fn send_prompt(&self, prompt: Seq, text: &str, attachments: &[AttachmentFile]) -> Result<(), AgentError>;
    async fn resolve_approval(&self, approval_id: &ApprovalId, decision: &ApprovalDecision) -> Result<(), AgentError>;
    async fn interrupt(&self) -> Result<(), AgentError>;
    /// Changes the model, approval mode or effort of the running process, and resolves once it has agreed. A setting
    /// that's `None` is left alone. An error with [`AgentProcess::is_gone`] false is a refusal, of the first setting
    /// refused, and the ones before it may already have been taken.
    async fn configure(&self, options: &SessionSettingsChange) -> Result<(), AgentError>;
    /// Asks the process to stop, and forces it if it doesn't. Resolves once it has exited.
    async fn stop(&self);
}

pub fn spawn(
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    match spec.agent {
        AgentKind::Claude => claude::spawn(spec, output, options),
        AgentKind::Unknown => Err(AgentError::UnknownKind),
    }
}

/// The account `kind`'s CLI is logged in to on this machine, from a one-off call that runs in `scratch_dir`.
pub async fn account(kind: AgentKind, options: &AgentOptions, scratch_dir: &Path) -> Result<AgentAccount, AgentError> {
    match kind {
        AgentKind::Claude => claude::account(options, scratch_dir).await,
        AgentKind::Unknown => Err(AgentError::UnknownKind),
    }
}

/// A short title for a prompt, from a one-off call to `kind`'s CLI that runs in an empty directory made inside
/// `scratch_dir`. `options.pid_file` records it while it runs.
pub async fn title_for(
    kind: AgentKind,
    prompt: &str,
    options: &AgentOptions,
    scratch_dir: &Path,
) -> Result<Title, AgentError> {
    match kind {
        AgentKind::Claude => claude::title_for(prompt, options, scratch_dir).await,
        AgentKind::Unknown => Err(AgentError::UnknownKind),
    }
}
