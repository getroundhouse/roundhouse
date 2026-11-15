use std::collections::HashMap;

use protocol::capability::calls::call::StartSubagent;
use protocol::model::{SubagentId, ToolUseId};

use crate::codex::state::{ThreadId, TurnId};

#[derive(Default)]
pub(in crate::codex) struct Subagents {
    pub(super) threads: HashMap<ThreadId, Child>,
    /// Each turn a subagent has been given, as a subagent of its own.
    pub(in crate::codex) runs: HashMap<SubagentId, Run>,
}

impl Subagents {
    /// The subagent working on `thread` now.
    pub(in crate::codex) fn subagent(&self, thread: &ThreadId) -> Option<SubagentId> {
        self.threads.get(thread).map(|child| child.current.clone())
    }
}

pub(super) struct Child {
    pub(super) current: SubagentId,
    /// Keep ended turns' identities: their late items must not enter a newer invocation's transcript.
    pub(super) turns: HashMap<TurnId, SubagentId>,
}

pub(in crate::codex) struct Run {
    /// The call that started it, which its result answers.
    pub(super) call: ToolUseId,
    pub(super) parent: Option<SubagentId>,
    pub(super) details: StartSubagent,
    pub(super) turn: Option<TurnId>,
    pub(in crate::codex) reply: Option<String>,
    pub(in crate::codex) ended: bool,
}
