use std::collections::HashMap;

use protocol::capability::calls::call::StartSubagent;
use protocol::model::{SubagentId, ToolUseId};

use crate::codex::state::{ThreadId, TurnId};

/// The subagents Codex has started, each named by its thread, which is one subagent however many turns it takes,
/// since a follow-up carries on with the same conversation.
#[derive(Default)]
pub(in crate::codex) struct Subagents {
    /// Each child thread's subagent, by the subagent.
    pub(in crate::codex) runs: HashMap<SubagentId, Run>,
}

impl ThreadId {
    /// The subagent working on this thread, which is named by it, if it's a child's.
    pub(in crate::codex) fn subagent(&self) -> SubagentId {
        self.0.as_str().into()
    }
}

pub(in crate::codex) struct Run {
    /// The call that started it, which its line in the chat is drawn for.
    pub(super) call: ToolUseId,
    pub(super) parent: Option<SubagentId>,
    pub(super) details: StartSubagent,
    /// The turn it's taking, or took last. Only that turn's end ends it, so one that ends late, once a follow-up has
    /// started, doesn't.
    pub(super) turn: Option<TurnId>,
    pub(in crate::codex) reply: Option<String>,
    pub(in crate::codex) ended: bool,
    /// Whether its call has had its result, which the end of its first turn gives it.
    pub(super) answered: bool,
}
