//! Asks a running agent what the hub asks.
use protocol::attachment::AttachmentFile;
use protocol::capability::Control;
use protocol::capability::prompts::Extras;
use protocol::model::Seq;
use protocol::prompt::Prompt;

use crate::{AgentError, AgentProcess};

/// What a test asks of a running agent, each sent as the control the hub sends for it, so a test goes through what the
/// hub's requests do, claims and all.
#[async_trait::async_trait]
pub trait Asking {
    /// The prompt recorded at `seq`.
    async fn prompt(&self, seq: Seq, prompt: &Prompt<AttachmentFile>) -> Result<(), AgentError>;
    /// Stops the turn.
    async fn cancel(&self) -> Result<(), AgentError>;
    /// A compaction recorded at `asked`, keeping what `keeping` says.
    async fn compact(&self, asked: Seq, keeping: Option<&str>) -> Result<(), AgentError>;
}

#[async_trait::async_trait]
impl<T: AgentProcess + ?Sized> Asking for T {
    async fn prompt(&self, seq: Seq, prompt: &Prompt<AttachmentFile>) -> Result<(), AgentError> {
        self.control(&Control::Prompt { seq, prompt: prompt.clone(), extras: Extras::default() }).await
    }

    async fn cancel(&self) -> Result<(), AgentError> {
        self.control(&Control::Cancel).await
    }

    async fn compact(&self, asked: Seq, keeping: Option<&str>) -> Result<(), AgentError> {
        self.control(&Control::Compact { asked, keeping: keeping.map(str::to_string) }).await
    }
}
