//! Agent adapters. Each one runs an agent as a child process and turns its output into normalized
//! events, while keeping every raw line so those events can be rebuilt later.

pub mod claude;

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use protocol::event::{ApprovalDecision, EventBody};
use protocol::model::AgentKind;
use protocol::tail::StreamDelta;
use protocol::worker::AgentSpec;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum AgentOutput {
    Event {
        body: EventBody,
        raw: Option<String>,
    },
    Delta(StreamDelta),
    /// Always the last thing sent.
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
        stderr_tail: Vec<String>,
        requested: bool,
    },
}

#[derive(Clone, Debug, Default)]
pub struct AgentOptions {
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
    /// An error means this process can't take the prompt, and [`AgentProcess::is_gone`] is now true.
    async fn send_prompt(&self, text: &str) -> anyhow::Result<()>;
    async fn resolve_approval(&self, approval_id: &str, decision: &ApprovalDecision) -> anyhow::Result<()>;
    async fn interrupt(&self) -> anyhow::Result<()>;
    /// Changes the model or approval mode of the running process, and resolves once it has agreed.
    /// `None` leaves that setting alone. An error with [`AgentProcess::is_gone`] false is a refusal.
    async fn configure(&self, model: Option<&str>, permission_mode: Option<&str>) -> anyhow::Result<()>;
    /// Asks the process to stop, and forces it if it doesn't. Resolves once it has exited.
    async fn stop(&self);
}

pub fn spawn(
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
) -> anyhow::Result<Arc<dyn AgentProcess>> {
    match spec.agent {
        AgentKind::Claude => claude::spawn(spec, output, options),
    }
}
