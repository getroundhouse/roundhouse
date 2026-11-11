//! The session event log, which is the record every client reads.
//!
//! Events are normalized so that clients don't parse agent formats. The worker also captures each
//! agent's raw output line, and the hub stores it beside the event, so that this view can be
//! rebuilt if the normalization turns out to be wrong.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::attachment::Attachment;
use crate::model::{
    AgentSessionId, ApprovalId, ClientMessageId, MessageId, ModelName, PermissionMode, Place, RunId, Seq, SessionId,
    TaskId, ToolUseId, UsageLimit,
};

pub use content::{ContentBlock, LinkedImage};
pub use file_change::{FileChange, Hunk, HunkLine, LineMark};
pub use tokens::TokenUsage;

mod content;
mod file_change;
mod tokens;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionEvent {
    pub session_id: SessionId,
    pub seq: Seq,
    pub ts_ms: i64,
    /// The launch of the agent that produced it. `None` for what the hub records itself, and for
    /// events recorded before runs were.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    /// The subagent it's from, as the call that started it, or `None` for the agent's own. The hub records it from
    /// [`EventBody::subagent`], or from the agent's line for an `AgentOther` recorded before that named its subagent. A
    /// hub that doesn't record it sends none, and `place` as 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent: Option<ToolUseId>,
    #[serde(default)]
    pub place: Place,
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
        /// The files it carries, in the order they were added. Their bytes are read with `AttachmentRead`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<Attachment>,
    },
    /// The agent took the prompt recorded at `prompt` into its conversation. One sent mid-turn waits until the agent
    /// can take it, such as when a running tool finishes. Every prompt gets this or `PromptDropped`, and until then the
    /// agent hasn't got it.
    PromptTaken {
        prompt: Seq,
    },
    /// The prompt recorded at `prompt` never reached the agent's conversation, and won't: its turn was stopped first,
    /// or the agent went away or couldn't be started. The agent says so, or the hub does when the agent can't.
    PromptDropped {
        prompt: Seq,
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
        permission_mode: Option<PermissionMode>,
        agent_version: Option<String>,
    },
    AgentState {
        state: AgentRunState,
    },
    /// Everything the agent is running in the background, whenever that changes, and empty once it's all done.
    AgentTasks {
        tasks: Vec<AgentTask>,
        /// Those that left the list because they were stopped rather than finishing, as they are when the agent
        /// itself is stopped.
        #[serde(default)]
        stopped: Vec<TaskId>,
    },
    /// How much of each limit on the account the agent runs under is used, as the agent just heard. A limit is the
    /// account's rather than the session's, so the hub keeps the newest for every session under that account.
    AgentLimits {
        limits: Vec<UsageLimit>,
    },
    /// The agent started a subagent with the call `call`, which the subagent's events name as their parent. It's the
    /// agent's own event, about the subagent.
    SubagentStarted {
        call: ToolUseId,
        /// Its kind of agent, such as `Explore`.
        #[serde(default)]
        kind: Option<String>,
        /// What it's for, in a few words.
        #[serde(default)]
        description: Option<String>,
        /// What it was asked.
        #[serde(default)]
        prompt: Option<String>,
        /// It works on while the agent carries on, rather than the call waiting for it.
        #[serde(default)]
        in_background: bool,
    },
    /// The subagent that `call` started has stopped working.
    SubagentEnded {
        call: ToolUseId,
        outcome: SubagentOutcome,
        /// What it reported, once it finished.
        #[serde(default)]
        report: Option<String>,
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
    /// The agent went on without a result for the call `tool_use_id`: whoever made it began their next message with
    /// something else. Nothing the agent sends says this, so its adapter does, from what came next, and it knows what
    /// that means for its agent. A result recorded after this stands, and the call was answered after all. A call its
    /// turn ended on gets none of these, since the turn's end says as much.
    ToolCallDropped {
        tool_use_id: ToolUseId,
    },
    TurnEnded {
        outcome: TurnOutcome,
        result: Option<String>,
        #[serde(default)]
        errors: Vec<String>,
        /// How long the turn took, by the agent's own count.
        duration_ms: Option<u64>,
        /// What the agent says its process has cost so far, which counts the turns before this one too.
        cost_usd: Option<f64>,
        num_turns: Option<u64>,
        /// When the turn started, by the hub's clock, which the hub adds as it records the end. So every turn is timed
        /// the way the working line counts it, and a client needn't load the turn's start to time it. `None` from the
        /// agent, which doesn't know it, and from a hub that doesn't add it.
        #[serde(default)]
        started_at_ms: Option<i64>,
        /// The models the agent's own replies in the turn came from, in the order they first did, its subagents'
        /// left out. Empty from an agent that doesn't say, and in a turn recorded before it was kept.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        models: Vec<ModelName>,
        /// How many tokens the agent's own requests in the turn took, its subagents' left out. `None` from an agent
        /// that doesn't say, and in a turn recorded before it was kept.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tokens: Option<TokenUsage>,
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
        /// What the person answered the agent's questions, as [`ApprovalDecision::Answer`] gave it, so the transcript
        /// can say what was decided. `None` for any other decision, and in a record from before the hub kept answers.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answers: Option<BTreeMap<String, String>>,
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
    /// The agent's approval mode changed, whoever changed it. It changes by itself too: approving a plan leaves plan
    /// mode. The hub records one when the mode is changed with no agent running, for the next agent to start in.
    AgentMode {
        permission_mode: PermissionMode,
    },
    /// An agent event with no normalized form. The stored raw line has the details.
    AgentOther {
        kind: String,
        /// The call that started the subagent it's from, if the agent said. It's `None` in one recorded before this
        /// was, whose `SessionEvent::subagent` says.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<ToolUseId>,
    },
    #[serde(other)]
    Unknown,
}

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
            EventBody::SubagentEnded { .. } => "subagent_ended",
            EventBody::Assistant { .. } => "assistant",
            EventBody::AgentUser { .. } => "agent_user",
            EventBody::ToolCallDropped { .. } => "tool_call_dropped",
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

    /// The subagent it's from, as the call that started it, or `None` for the agent's own. Only what the agent sends
    /// can come from a subagent, and only these say which.
    pub fn subagent(&self) -> Option<&ToolUseId> {
        match self {
            EventBody::Assistant { parent_tool_use_id, .. }
            | EventBody::AgentUser { parent_tool_use_id, .. }
            | EventBody::AgentOther { parent_tool_use_id, .. } => parent_tool_use_id.as_ref(),
            EventBody::UserPrompt { .. }
            | EventBody::PromptTaken { .. }
            | EventBody::PromptDropped { .. }
            | EventBody::AgentStarted { .. }
            | EventBody::AgentInit { .. }
            | EventBody::AgentState { .. }
            | EventBody::AgentTasks { .. }
            | EventBody::AgentLimits { .. }
            | EventBody::ToolCallDropped { .. }
            | EventBody::SubagentStarted { .. }
            | EventBody::SubagentEnded { .. }
            | EventBody::TurnEnded { .. }
            | EventBody::ApprovalRequested { .. }
            | EventBody::ApprovalResolved { .. }
            | EventBody::AgentExited { .. }
            | EventBody::Notice { .. }
            | EventBody::AgentMode { .. }
            | EventBody::Unknown => None,
        }
    }
}

/// Work an agent started in the background, such as a command or a subagent. It carries on after the turn that
/// started it ends.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentTask {
    pub id: TaskId,
    /// What the agent said it's for.
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub kind: TaskKind,
}

/// What a background task is, which says whether it's the agent's own work and whether it's doing anything.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// A subagent, which is the agent's own work.
    Subagent,
    /// A command that watches for something, and wakes the agent each time it happens. It does nothing meanwhile.
    Monitor,
    /// A command the agent left running. It might end by itself, as a test run does, or be held open, as an app is,
    /// and the agent doesn't say which. It's also what a list from before kinds were kept holds, and what a kind
    /// this build doesn't know reads as.
    #[default]
    #[serde(other)]
    Command,
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

/// How a subagent stopped working. One this build doesn't know reads as `Unknown`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubagentOutcome {
    Finished,
    Failed,
    /// The agent stopped it, or it went with the agent.
    Stopped,
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
