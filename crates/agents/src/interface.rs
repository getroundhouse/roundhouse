use crate::AgentError;
use async_trait::async_trait;
use protocol::attachment::AttachmentFile;
use protocol::model::Seq;
use protocol::prompt::Prompt;

#[async_trait]
pub trait AgentProcess: Send + Sync {
    fn capabilities(&self) -> protocol::capability::Capabilities;
    /// Does what `control` says, which the hub has checked against this run's claims. The methods below are what the
    /// adapters carry a control out with, and what the conformance driver and the adapters' tests still call, until
    /// #1275 leaves only this and `stop`. A decision comes only as a control.
    async fn control(&self, control: &protocol::capability::Control) -> Result<(), AgentError>;
    fn pid(&self) -> u32;
    /// The process has exited, or can't be reached. A new prompt needs a new process, even though
    /// its last output may still be on the way.
    fn is_gone(&self) -> bool;
    /// An error means this process didn't take the prompt. Only [`AgentProcess::is_gone`] permits restarting it. The prompt can
    /// say nothing when it carries files, which are in the worktree already. `seq` is where the prompt is recorded,
    /// which the process names when it reports taking or dropping it.
    async fn send_prompt(&self, seq: Seq, prompt: &Prompt<AttachmentFile>) -> Result<(), AgentError>;
    async fn interrupt(&self) -> Result<(), AgentError>;
    /// Asks the agent to compact its conversation, keeping what `keeping` says, in whatever way it's asked. `asked` is
    /// where that's recorded, which the process names when it reports taking or dropping it, and on the compaction.
    /// An error means what it does for [`AgentProcess::send_prompt`].
    async fn compact(&self, asked: Seq, keeping: Option<&str>) -> Result<(), AgentError>;
    /// Asks the process to stop, and forces it if it doesn't. Resolves once it has exited.
    async fn stop(&self);
}
