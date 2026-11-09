//! The session event log, which is the record every client reads.
//!
//! Events are normalized so that clients don't parse agent formats. The worker also captures each
//! agent's raw output line, and the hub stores it beside the event, so that this view can be
//! rebuilt if the normalization turns out to be wrong.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{AgentSessionId, ApprovalId, ClientMessageId, MessageId, RunId, Seq, SessionId, ToolUseId};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionEvent {
    pub session_id: SessionId,
    pub seq: Seq,
    pub ts_ms: i64,
    /// The launch of the agent that produced it. `None` for what the hub records itself, and for
    /// events recorded before runs were.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    pub body: EventBody,
    /// The agent's original line. Only sent when a read asks for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventBody {
    /// A prompt becomes real once this is recorded. Until then a client shows it as pending, and
    /// matches it here by `client_msg_id`.
    UserPrompt {
        text: String,
        #[serde(default)]
        client_msg_id: Option<ClientMessageId>,
    },
    /// The worker launched an agent process for this session.
    AgentStarted {
        pid: u32,
        resumed: bool,
    },
    /// The agent reported its configuration. Claude does this at the start of every turn.
    AgentInit {
        agent_session_id: Option<AgentSessionId>,
        model: Option<String>,
        cwd: Option<String>,
        permission_mode: Option<String>,
        agent_version: Option<String>,
    },
    AgentState {
        state: AgentRunState,
    },
    /// Claude sends one of these per content block, sharing `message_id`.
    Assistant {
        message_id: Option<MessageId>,
        #[serde(default)]
        parent_tool_use_id: Option<ToolUseId>,
        model: Option<String>,
        blocks: Vec<ContentBlock>,
        /// Where the first of `blocks` sits in its message, as the deltas that streamed it numbered it,
        /// with the rest after it in order. A block keeps the identity it had while streaming. `None`
        /// when nothing streamed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        first_index: Option<u32>,
    },
    /// Content the agent itself added on the user's side: tool results and interruption notes.
    AgentUser {
        #[serde(default)]
        parent_tool_use_id: Option<ToolUseId>,
        blocks: Vec<ContentBlock>,
    },
    TurnEnded {
        outcome: TurnOutcome,
        result: Option<String>,
        #[serde(default)]
        errors: Vec<String>,
        duration_ms: Option<u64>,
        cost_usd: Option<f64>,
        num_turns: Option<u64>,
    },
    ApprovalRequested {
        approval_id: ApprovalId,
        tool_name: String,
        tool_use_id: Option<ToolUseId>,
        input: Value,
        description: Option<String>,
        /// Agent-defined options for remembering the decision. Passed back unchanged.
        #[serde(default)]
        suggestions: Value,
    },
    ApprovalResolved {
        approval_id: ApprovalId,
        outcome: ApprovalOutcome,
    },
    AgentExited {
        reason: ExitReason,
        code: Option<i32>,
        signal: Option<i32>,
        #[serde(default)]
        stderr_tail: Vec<String>,
    },
    /// Something the user should see that isn't from the agent.
    Notice {
        level: NoticeLevel,
        message: String,
    },
    /// An agent event with no normalized form. The stored raw line has the details.
    /// The agent's approval mode changed, whoever changed it. It changes by itself too: approving a
    /// plan leaves plan mode.
    AgentMode {
        permission_mode: String,
    },
    AgentOther {
        kind: String,
    },
    #[serde(other)]
    Unknown,
}

impl EventBody {
    /// Stored beside the event, so the hub can find events by kind without parsing the log.
    pub fn kind(&self) -> &'static str {
        match self {
            EventBody::UserPrompt { .. } => "user_prompt",
            EventBody::AgentStarted { .. } => "agent_started",
            EventBody::AgentInit { .. } => "agent_init",
            EventBody::AgentState { .. } => "agent_state",
            EventBody::Assistant { .. } => "assistant",
            EventBody::AgentUser { .. } => "agent_user",
            EventBody::TurnEnded { .. } => "turn_ended",
            EventBody::ApprovalRequested { .. } => "approval_requested",
            EventBody::ApprovalResolved { .. } => "approval_resolved",
            EventBody::AgentExited { .. } => "agent_exited",
            EventBody::Notice { .. } => "notice",
            EventBody::AgentMode { .. } => "agent_mode",
            EventBody::AgentOther { .. } => "agent_other",
            EventBody::Unknown => "unknown",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Thinking {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: ToolUseId,
        is_error: bool,
        text: String,
        /// Images aren't copied into the normalized event. They stay in the raw line.
        #[serde(default)]
        image_count: u32,
    },
    Image {
        media_type: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunState {
    Idle,
    Running,
    RequiresAction,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    Success,
    Cancelled,
    Error,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalOutcome {
    Allowed,
    Denied,
    /// Nobody answered: the turn was interrupted, or the agent went away.
    Cancelled,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ApprovalDecision {
    Allow {
        /// Replaces the tool's input. `None` runs it as requested.
        #[serde(default)]
        updated_input: Option<Value>,
        /// Agent-defined rules to remember, taken from `suggestions`.
        #[serde(default)]
        updated_permissions: Option<Value>,
    },
    Deny {
        message: String,
        /// Also stops the turn rather than letting the agent try something else.
        #[serde(default)]
        interrupt: bool,
    },
    /// The agent asked the user something, and this is what they said, by the question's text. Several
    /// choices for one question are joined with a comma and a space.
    Answer { answers: BTreeMap<String, String> },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExitReason {
    /// The hub asked the agent to stop.
    Requested,
    /// The agent exited by itself.
    Unexpected,
    /// The worker process went away. The provider stops anything it left running.
    WorkerLost,
    /// The hub went away while the agent was running. Recorded when the hub next starts.
    HubRestart,
    /// The agent could not be started.
    StartFailed,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
    #[serde(other)]
    Unknown,
}
