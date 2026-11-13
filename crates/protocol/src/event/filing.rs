//! How an event is filed: the kind the hub stores it under, and the subagent it's from.

use super::EventBody;
use crate::model::ToolUseId;

impl EventBody {
    /// Stored beside the event, so the hub can find events by kind without parsing the log.
    pub fn kind(&self) -> &'static str {
        match self {
            EventBody::UserPrompt { .. } => "user_prompt",
            EventBody::PromptTaken { .. } => "prompt_taken",
            EventBody::PromptDropped { .. } => "prompt_dropped",
            EventBody::AgentStarted { .. } => "agent_started",
            EventBody::AgentInit { .. } => "agent_init",
            EventBody::AgentState { .. } => "agent_state",
            EventBody::AgentTasks { .. } => "agent_tasks",
            EventBody::AgentLimits { .. } => "agent_limits",
            EventBody::SubagentStarted { .. } => "subagent_started",
            EventBody::SubagentContext { .. } => "subagent_context",
            EventBody::SubagentEnded { .. } => "subagent_ended",
            EventBody::Assistant { .. } => "assistant",
            EventBody::AgentUser { .. } => "agent_user",
            EventBody::ToolCallDropped { .. } => "tool_call_dropped",
            EventBody::TurnEnded { .. } => "turn_ended",
            EventBody::RequestFailed { .. } => "request_failed",
            EventBody::ApprovalRequested { .. } => "approval_requested",
            EventBody::ApprovalResolved { .. } => "approval_resolved",
            EventBody::AgentExited { .. } => "agent_exited",
            EventBody::Notice { .. } => "notice",
            EventBody::AgentMode { .. } => "agent_mode",
            EventBody::AgentOther { .. } => "agent_other",
            EventBody::CompactionAsked { .. } => "compaction_asked",
            EventBody::AgentCompacting { .. } => "agent_compacting",
            EventBody::AgentCompacted { .. } => "agent_compacted",
            EventBody::CacheExpired { .. } => "cache_expired",
            EventBody::Unknown => "unknown",
        }
    }

    /// The subagent it's from, as the call that started it, or `None` for the agent's own. Only what the agent and its
    /// CLI send can come from a subagent, and only these say which.
    pub fn subagent(&self) -> Option<&ToolUseId> {
        match self {
            EventBody::Assistant { parent_tool_use_id, .. }
            | EventBody::AgentUser { parent_tool_use_id, .. }
            | EventBody::AgentOther { parent_tool_use_id, .. }
            | EventBody::RequestFailed { parent_tool_use_id, .. }
            | EventBody::Notice { parent_tool_use_id, .. } => parent_tool_use_id.as_ref(),
            EventBody::UserPrompt { .. }
            | EventBody::CompactionAsked { .. }
            | EventBody::PromptTaken { .. }
            | EventBody::PromptDropped { .. }
            | EventBody::AgentStarted { .. }
            | EventBody::AgentInit { .. }
            | EventBody::AgentState { .. }
            | EventBody::AgentTasks { .. }
            | EventBody::AgentLimits { .. }
            | EventBody::ToolCallDropped { .. }
            | EventBody::SubagentStarted { .. }
            | EventBody::SubagentContext { .. }
            | EventBody::SubagentEnded { .. }
            | EventBody::TurnEnded { .. }
            | EventBody::ApprovalRequested { .. }
            | EventBody::ApprovalResolved { .. }
            | EventBody::AgentExited { .. }
            | EventBody::AgentMode { .. }
            | EventBody::AgentCompacting { .. }
            | EventBody::AgentCompacted { .. }
            | EventBody::CacheExpired { .. }
            | EventBody::Unknown => None,
        }
    }
}
