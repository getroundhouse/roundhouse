use std::collections::HashMap;

use protocol::capability::calls::call::StartSubagent;
use protocol::model::ToolUseId;

use crate::codex::state::{ThreadId, TurnId};

#[derive(Default)]
pub(in crate::codex) struct Subagents {
    pub(super) threads: HashMap<ThreadId, Child>,
    pub(in crate::codex) runs: HashMap<ToolUseId, Run>,
}

impl Subagents {
    pub(in crate::codex) fn call(&self, thread: &ThreadId) -> Option<ToolUseId> {
        self.threads.get(thread).map(|child| child.current.clone())
    }
}

pub(super) struct Child {
    pub(super) current: ToolUseId,
    /// Keep ended turns' identities: their late items must not enter a newer invocation's transcript.
    pub(super) turns: HashMap<TurnId, ToolUseId>,
}

pub(in crate::codex) struct Run {
    pub(super) parent: Option<ToolUseId>,
    pub(super) details: StartSubagent,
    pub(super) turn: Option<TurnId>,
    pub(in crate::codex) reply: Option<String>,
    pub(in crate::codex) ended: bool,
}
