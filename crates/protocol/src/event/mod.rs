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
use crate::prompt::Prompt;

pub use approval::{ApprovalDecision, ApprovalOutcome};
pub use asking::{Asking, Question, QuestionAnswer, QuestionOption};
pub use call::CallKind;
pub use compaction::{CompactedBy, CompactionOutcome};
pub use content::{ContentBlock, Denied, LinkedImage};
pub use ending::{ExitReason, SubagentOutcome, TurnOutcome};
pub use failure::{FailureKind, RequestFailure};
pub use file_change::{FileChange, Hunk, HunkLine, LineMark};
pub use task::{AgentTask, TaskKind};
pub use tokens::TokenUsage;

mod approval;
mod asking;
mod call;
mod compaction;
mod content;
mod ending;
mod failure;
mod file_change;
mod filing;
mod task;
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
        #[serde(default)]
        client_msg_id: Option<ClientMessageId>,
        /// Its files are in the order they were added, and their bytes are read with `AttachmentRead`.
        #[serde(flatten)]
        prompt: Prompt<Attachment>,
    },
    /// The agent took the prompt recorded at `prompt` into its conversation. One sent mid-turn waits until the agent
    /// can take it, such as when a running tool finishes. Every prompt gets this or `PromptDropped`, and until then the
    /// agent hasn't got it. So does a `CompactionAsked`, which is named the same way.
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
        /// Which build the agent runs on, which is its worker's, as the worker's hello said. `None` from a worker from
        /// before workers said, and from a hub from before it recorded this.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        build: Option<String>,
        /// The build the chat's agent last ran on, when that's another than `build`, as after an update or a rollback,
        /// which is where a chat says it moved. `None` when it's the same build, or either isn't known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        previous_build: Option<String>,
    },
    /// The agent reported its configuration. Claude does this at the start of every turn.
    AgentInit {
        agent_session_id: Option<AgentSessionId>,
        /// As the agent said it, which is untrusted. A [`ModelName`] refuses some names as it's read, so one in the log
        /// would fail every read that reached this event. Where a model is kept as one, one that names none is dropped.
        model: Option<String>,
        cwd: Option<String>,
        permission_mode: Option<PermissionMode>,
        agent_version: Option<String>,
    },
    AgentState {
        state: AgentRunState,
    },
    /// Everything the agent is running in the background, whenever that changes, and empty once it's all done. The hub
    /// records one too, to take waits on the pull request off the list once they're over with no agent running to.
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
    /// How much the conversation of the subagent that `call` started holds, as the agent says while it works. It's the
    /// agent's own event, about the subagent.
    SubagentContext {
        call: ToolUseId,
        /// As [`Subagent::context_tokens`](crate::model::Subagent::context_tokens) counts them.
        tokens: u64,
    },
    /// The subagent that `call` started has stopped working.
    SubagentEnded {
        call: ToolUseId,
        outcome: SubagentOutcome,
        /// What it reported, once it finished.
        #[serde(default)]
        report: Option<String>,
        /// How much its conversation held as it ended, as [`EventBody::SubagentContext`] counts it. `None` from an
        /// agent that doesn't say, and in one recorded before it was kept.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_tokens: Option<u64>,
    },
    /// Claude sends one of these per content block, sharing `message_id`.
    Assistant {
        message_id: Option<MessageId>,
        #[serde(default)]
        parent_tool_use_id: Option<ToolUseId>,
        /// As the agent said it, for the reason [`EventBody::AgentInit`]'s is.
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
        /// What the agent says the conversation has cost so far, across every launch, which counts the turns before this
        /// one too.
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
        /// The turn was only a compaction someone asked for, so the compaction says all there is to say of it. The
        /// agent's adapter says, since how a compaction is asked, and whether it's a turn of its own, is the agent's.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        only_compacted: bool,
        /// The turn failed because one of the agent's own requests to its model did, which the
        /// [`EventBody::RequestFailed`] before it says, so the turn's end has nothing to add. The agent's adapter says,
        /// since only it knows that its agent ends a turn there.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        request_failed: bool,
    },
    /// One of the agent's requests to its model failed, and its CLI said why in a message of its own, in the place of
    /// the model's reply. The agent's turn mostly ends there, failed, but a subagent's failing leaves the agent going.
    RequestFailed {
        /// The call that started the subagent whose request it was, or `None` for the agent's own.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<ToolUseId>,
        failure: RequestFailure,
    },
    ApprovalRequested {
        approval_id: ApprovalId,
        tool_name: String,
        tool_use_id: Option<ToolUseId>,
        input: Value,
        description: Option<String>,
        /// Agent-defined options for remembering the decision. Passed back unchanged. Boxed, as `asking` is, since a
        /// request says more than any other event, and every event takes the room of the largest.
        #[serde(default)]
        suggestions: Box<Value>,
        /// What it asks of the person, as the agent's adapter read it from the tool and its input. `None` in one
        /// recorded before adapters said, which a client reads from the tool and input as Claude's adapter sent them.
        /// Boxed, since few events are approvals, and every event takes the room of the largest.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asking: Option<Box<Asking>>,
        /// Why the agent asks, in its own words, when it says: a check it makes whatever the approval mode, such as
        /// Claude's for a removal it can't rule out, says so here. `None` from an agent that doesn't say, and in one
        /// recorded before it was kept.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        /// The subagent that asks, as the call that started it, or `None` for the agent itself, and in one recorded
        /// before adapters said. The request is among the agent's own events whichever asks, as
        /// [`EventBody::subagent`] says, since it waits on the person, and a chat reads the agent's own.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asked_by: Option<ToolUseId>,
    },
    ApprovalResolved {
        approval_id: ApprovalId,
        outcome: ApprovalOutcome,
        /// What the person answered the agent's questions, by their text as [`ApprovalDecision::AnswerByText`] gave it,
        /// or by position as [`ApprovalDecision::AnswerByPosition`] did, so the transcript can say what was decided.
        /// Each is `None` for any other decision, and both are in a record from before the hub kept answers.
        #[serde(rename = "answers", default, skip_serializing_if = "Option::is_none")]
        answers_by_text: Option<BTreeMap<String, String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answers_by_position: Option<Vec<QuestionAnswer>>,
        /// The hub allowed it itself, as its settings have it allow what an agent with full access still asks about,
        /// rather than the person deciding. False for everything else, and in a record from before the hub could.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        by_setting: bool,
    },
    AgentExited {
        reason: ExitReason,
        code: Option<i32>,
        signal: Option<i32>,
        #[serde(default)]
        stderr_tail: Vec<String>,
    },
    /// Something the user should see that isn't the agent's words: the hub's, or the agent's CLI's, such as what a
    /// command it ran for the person printed. Its message can hold Markdown, as the CLI's can.
    Notice {
        level: NoticeLevel,
        message: String,
        /// The call that started the subagent the CLI said it of, or `None`. The hub's are never a subagent's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<ToolUseId>,
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
    /// The agent is compacting its conversation: writing a summary to carry on from in place of what it holds, so its
    /// context has room again. It says so again every so often until it's done.
    AgentCompacting {
        /// How much of the summary is written so far, in characters, when the agent says.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        written_chars: Option<u64>,
    },
    /// Someone asked the agent to compact its conversation now, rather than when its context fills. The hub records it
    /// only while the agent isn't on a turn, and the agent takes it or drops it as it does a prompt, with
    /// `PromptTaken` or `PromptDropped` naming it. How the agent is asked is its adapter's to know.
    CompactionAsked {
        /// What to keep, in the asker's own words, when they said.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keeping: Option<String>,
    },
    /// The agent compacted its conversation, or couldn't.
    AgentCompacted {
        /// The `CompactionAsked` it answers, as the agent's adapter knows. `None` for one the agent started itself, and
        /// for one a prompt asked for in the agent's own way, such as a typed `/compact` from a client that doesn't ask
        /// the hub.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asked: Option<Seq>,
        /// What that said to keep, so the compaction can say it without its request.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keeping: Option<String>,
        #[serde(default)]
        outcome: CompactionOutcome,
    },
    /// The first request for a prompt found the API's cache of the conversation expired, and wrote it again. The hub
    /// records it from what the agent's adapter says of each request, since only the hub knows when the request before
    /// it was sent, whichever process sent it. A request to another model writes it again too, which isn't this.
    CacheExpired {
        /// How long it was since the request before it was sent.
        #[serde(default)]
        idle_ms: u64,
        /// How much it read, which is what every chat shares, such as the tools.
        #[serde(default)]
        read_tokens: u64,
        /// How much it wrote again.
        #[serde(default)]
        written_tokens: u64,
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
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
    #[serde(other)]
    Unknown,
}
