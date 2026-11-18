use crate::AgentError;
use async_trait::async_trait;

#[async_trait]
pub trait AgentProcess: Send + Sync {
    fn capabilities(&self) -> protocol::capability::Capabilities;
    /// Does what `control` says, which the hub has checked against this run's claims, and refuses one it doesn't claim.
    /// Everything asked of a running agent comes as one, but for stopping it. An error from a prompt or a compaction
    /// means this process didn't take it, and only [`AgentProcess::is_gone`] permits starting another for it. A prompt
    /// can say nothing when it carries files, which are in the worktree already, and its `seq`, and a compaction's
    /// `asked`, are where it's recorded, which the process names when it reports taking or dropping it.
    async fn control(&self, control: &protocol::capability::Control) -> Result<(), AgentError>;
    fn pid(&self) -> u32;
    /// The process has exited, or can't be reached. A new prompt needs a new process, even though
    /// its last output may still be on the way.
    fn is_gone(&self) -> bool;
    /// Asks the process to stop, and forces it if it doesn't. Resolves once it has exited.
    async fn stop(&self);
}
