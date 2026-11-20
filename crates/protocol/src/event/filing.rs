//! How an event is filed: the kind the hub stores it under, and the subagent it's from.

use super::EventBody;
use crate::model::SubagentId;

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
            EventBody::TaskChanged { .. } => "task_changed",
            EventBody::OwnTurn(_) => "own_turn",
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
            EventBody::ConversationStarted { .. } => "conversation_started",
            EventBody::VoiceCut { .. } => "voice_cut",
            EventBody::Unknown => "unknown",
        }
    }

    /// The subagent it says started, if it says one did, by the ID its agent gives it, or by its call in one recorded
    /// before subagents had IDs of their own.
    pub fn subagent_started(&self) -> Option<SubagentId> {
        let EventBody::SubagentStarted { call, subagent, .. } = self else { return None };
        Some(subagent.clone().unwrap_or_else(|| SubagentId::of_call(call)))
    }

    /// The subagent it's from, or `None` for the agent's own. Only what the agent and its CLI send can come from a
    /// subagent, and only these say which.
    pub fn subagent(&self) -> Option<&SubagentId> {
        match self {
            EventBody::Assistant { subagent, .. }
            | EventBody::AgentUser { subagent, .. }
            | EventBody::AgentOther { subagent, .. }
            | EventBody::RequestFailed { subagent, .. }
            | EventBody::Notice { subagent, .. } => subagent.as_ref(),
            EventBody::UserPrompt { .. }
            | EventBody::CompactionAsked { .. }
            | EventBody::PromptTaken { .. }
            | EventBody::PromptDropped { .. }
            | EventBody::AgentStarted { .. }
            | EventBody::AgentInit { .. }
            | EventBody::AgentState { .. }
            | EventBody::AgentTasks { .. }
            | EventBody::TaskChanged { .. }
            | EventBody::OwnTurn(_)
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
            | EventBody::ConversationStarted { .. }
            | EventBody::VoiceCut { .. }
            | EventBody::Unknown => None,
        }
    }
}
