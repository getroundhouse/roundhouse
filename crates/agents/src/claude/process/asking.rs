//! What the adapter's own questions to the CLI as it works have in common: which of the agent's events say it's
//! working, and when asking is over for good.

use protocol::event::EventBody;

use crate::AgentError;

/// What an event says of the agent's work, for whatever asks the CLI something as it goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Work {
    /// A turn has started.
    Started,
    /// The agent wrote something. `subagent` when it was one of its subagents, which works in a context of its own.
    Wrote { subagent: bool },
    /// A turn has ended.
    TurnEnded,
    /// The CLI said by itself what's left of its account's limits.
    HeardLimits,
    /// Nothing that says the agent is working.
    Other,
}

pub(super) fn work(body: &EventBody) -> Work {
    match body {
        EventBody::AgentInit { .. } => Work::Started,
        EventBody::Assistant { parent_tool_use_id, .. } => Work::Wrote { subagent: parent_tool_use_id.is_some() },
        EventBody::TurnEnded { .. } => Work::TurnEnded,
        EventBody::AgentLimits { .. } => Work::HeardLimits,
        EventBody::UserPrompt { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::AgentUser { .. }
        | EventBody::ApprovalRequested { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::AgentExited { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentMode { .. }
        | EventBody::SubagentStarted { .. }
        | EventBody::SubagentEnded { .. }
        | EventBody::AgentOther { .. }
        | EventBody::Unknown => Work::Other,
    }
}

/// Why nothing more is asked or reported, once that's so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Over {
    /// The CLI doesn't know the request, so there's nothing to ask.
    Unsupported,
    /// The process has exited and said so, and nothing is reported after that.
    Exited,
}

impl Over {
    /// Whether an ask that failed with `error` means asking is over: only for a CLI that doesn't know the request,
    /// since any other failure may not come twice.
    pub(super) fn after(error: &AgentError) -> Option<Over> {
        matches!(error, AgentError::UnknownRequest(_)).then_some(Over::Unsupported)
    }
}
