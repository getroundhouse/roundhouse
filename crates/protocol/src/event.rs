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
    AgentSessionId, ApprovalId, ClientMessageId, MessageId, RunId, Seq, SessionId, TaskId, ToolUseId, UsageLimit,
};

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
        permission_mode: Option<String>,
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
        cost_usd: Option<f64>,
        num_turns: Option<u64>,
        /// When the turn started, by the hub's clock, which the hub adds as it records the end. So every turn is timed
        /// the way the working line counts it, and a client needn't load the turn's start to time it. `None` from the
        /// agent, which doesn't know it, and from a hub that doesn't add it.
        #[serde(default)]
        started_at_ms: Option<i64>,
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
            EventBody::PromptTaken { .. } => "prompt_taken",
            EventBody::PromptDropped { .. } => "prompt_dropped",
            EventBody::AgentStarted { .. } => "agent_started",
            EventBody::AgentInit { .. } => "agent_init",
            EventBody::AgentState { .. } => "agent_state",
            EventBody::AgentTasks { .. } => "agent_tasks",
            EventBody::AgentLimits { .. } => "agent_limits",
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
}

/// Work an agent started in the background, such as a command or a subagent. It carries on after the turn that
/// started it ends.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentTask {
    pub id: TaskId,
    /// What the agent said it's for.
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
        /// The images the text points to in the worktree, as they were when it came, each by the address the text
        /// gives it. The worker reads each, and the hub keeps it as an attachment of the session, so a client reads
        /// its bytes with `AttachmentRead`, as it does a prompt's. One that wasn't there, or wasn't an image inside the
        /// worktree, isn't named, and neither is any in a text recorded before the hub kept them.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<LinkedImage>,
    },
    Thinking {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// What a call sent back. It comes in an `AgentUser` event, except for a tool the API runs itself, such as Claude's
    /// advisor, whose result comes in an `Assistant` event.
    ToolResult {
        tool_use_id: ToolUseId,
        is_error: bool,
        text: String,
        /// Images the tool sent back, such as a screenshot the agent read. Each is an attachment of the session, which
        /// the hub keeps, so a client reads its bytes with `AttachmentRead`, as it does a prompt's. A result recorded
        /// before the hub kept them names none.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<Attachment>,
        /// What the tool changed in a file, when the agent says, as with Claude's edits and writes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        change: Option<FileChange>,
        /// What came back is encrypted, for the model alone, as the advisor's reply can be, so `text` holds none of it.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        encrypted: bool,
    },
    Image {
        media_type: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

/// An image a text points to, and the attachment that keeps it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LinkedImage {
    /// Where the text says the image is, as its Markdown gives it, before any decoding.
    pub address: String,
    pub attachment: Attachment,
}

/// What a tool changed in a file, as its agent reported it, so a client can show the change with the file's own line
/// numbers rather than work it out from the call. A kind this build doesn't know reads as `Unknown`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileChange {
    /// The file didn't exist before. What it holds is in the call that wrote it.
    Created,
    /// The file was there, and these parts of it changed, in order.
    Changed { hunks: Vec<Hunk> },
    #[serde(other)]
    Unknown,
}

/// One changed part of a file, as a unified diff's hunk has it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    /// The line the hunk starts on in the file before, from 1.
    pub old_start: u32,
    /// The line the hunk starts on in the file after, from 1.
    pub new_start: u32,
    pub lines: Vec<HunkLine>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HunkLine {
    pub mark: LineMark,
    /// The line, without the mark a unified diff puts before it.
    pub text: String,
}

/// Whether a line of a hunk was kept, removed or added. A mark this build doesn't know reads as `Unknown`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineMark {
    /// In the file both before and after.
    Same,
    Removed,
    Added,
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
