use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use protocol::capability::{calls::Call, settings, turns::OwnTurn};
use protocol::event::{AgentTask, EndedTask, TaskKind, TaskOutcome, TokenUsage};
use protocol::model::{AgentSessionId, ApprovalId, ModelName, PermissionMode, Seq, TaskId, ToolUseId};
use serde_json::Value;

use crate::acp::{ConfigOption, FieldSpec, PermissionRequest};

pub(super) struct Conversation {
    pub(super) session_id: Option<AgentSessionId>,
    /// `agy`'s trajectory log for this conversation and the byte offset at launch, for noticing between-turn wakeups.
    pub(super) transcript: Option<(PathBuf, u64)>,
    pub(super) model: ModelName,
    pub(super) mode: PermissionMode,
    pub(super) effort: settings::Effort,
    pub(super) hold_plans: bool,
    pub(super) config_options: Vec<ConfigOption>,
    /// Ordered `session/prompt` RPCs still awaiting their JSON-RPC completion. A mid-turn steer appends a second entry;
    /// `agy` resolves the superseded RPC first with `stopReason: "end_turn"` and keeps the turn open under the newer ID.
    pub(super) prompts: VecDeque<InFlightPrompt>,
    pub(super) ending: Ending,
    pub(super) cancelling: bool,
    /// What the turn's earlier prompts took, since `agy` reports each `session/prompt`'s own: a plan's, which its
    /// approval or feedback carries on in a prompt of its own. A steer's superseded prompt reports nothing, since the
    /// steering one reports the whole turn's, as `docs/antigravity-cli.md` says.
    pub(super) earlier_tokens: Option<TokenUsage>,
    pub(super) streaming_thought: Option<(String, String)>,
    pub(super) streaming_text: Option<(String, String)>,
    pub(super) reply: Option<String>,
    pub(super) next_message: u64,
    pub(super) next_call_order: u64,
    /// Open calls emitted as `ContentBlock::Call` and not yet settled by `CallResult` or `ToolCallDropped`.
    pub(super) calls: HashMap<ToolUseId, OpenCall>,
    /// Every call ID already emitted as `ContentBlock::Call` in this run, so repeated `tool_call` / `tool_call_update`
    /// notifications for the same tool call never emit a second `Call` block.
    pub(super) emitted_calls: HashSet<ToolUseId>,
    pub(super) denied_calls: HashSet<ToolUseId>,
    pub(super) listed_tasks: Vec<AgentTask>,
    pub(super) ended_tasks: Vec<EndedTask>,
    pub(super) waiting: HashMap<ApprovalId, Waiting>,
}

pub(super) struct OpenCall {
    pub(super) call: Call,
    pub(super) order: u64,
    /// Whether `agy` can keep this call running in the background after the current `session/prompt` finishes.
    pub(super) outlives_turn: bool,
}

/// How far the turn has got to ending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Ending {
    /// It hasn't begun to, or there's no turn.
    Not,
    /// Its last prompt has finished, and its end, or its plan, is being recorded.
    Recording,
    /// Its end waits on the person's decision on its plan.
    HeldForPlan,
}

#[derive(Clone, Debug)]
pub(super) struct InFlightPrompt {
    pub(super) rpc_id: u64,
    pub(super) origin: PromptOrigin,
    pub(super) taken: bool,
    /// Remaining non-empty `"text"` blocks in this prompt's request for which `agy` emits a `user_message_chunk`.
    pub(super) user_chunks_left: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PromptOrigin {
    /// A user prompt (`Some(seq)`) or an internal continuation such as plan approval (`None`).
    Prompt(Option<Seq>),
    /// An internal `session/prompt` sent to drain `agy` 1.5.9's buffered `<SYSTEM_MESSAGE>` wakeup updates.
    WakeupDrain { keep_open: bool, started_own_turn: bool, self_cancelled: bool },
}

impl InFlightPrompt {
    pub(super) fn seq(&self) -> Option<Seq> {
        match self.origin {
            PromptOrigin::Prompt(seq) => seq,
            PromptOrigin::WakeupDrain { .. } => None,
        }
    }

    pub(super) fn is_drain(&self) -> bool {
        matches!(self.origin, PromptOrigin::WakeupDrain { .. })
    }

    pub(super) fn started_own_turn(&self) -> bool {
        matches!(self.origin, PromptOrigin::WakeupDrain { started_own_turn: true, .. })
    }

    pub(super) fn self_cancelled(&self) -> bool {
        matches!(self.origin, PromptOrigin::WakeupDrain { self_cancelled: true, .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum UserChunk {
    Ignore,
    FirstOfPrompt(Option<Seq>),
    CancelDrain { session_id: AgentSessionId },
    ContinueDrain,
}

pub(super) struct Waiting {
    pub(super) native: NativeRequest,
}

pub(super) enum NativeRequest {
    Permission { id: Value, request: Box<PermissionRequest> },
    Elicitation { id: Value, fields: Vec<FieldSpec> },
    Plan,
}

impl Conversation {
    pub(super) fn new(model: ModelName, mode: PermissionMode, effort: settings::Effort, hold_plans: bool) -> Self {
        Self {
            session_id: None,
            transcript: None,
            model,
            mode,
            effort,
            hold_plans,
            config_options: Vec::new(),
            prompts: VecDeque::new(),
            ending: Ending::Not,
            cancelling: false,
            earlier_tokens: None,
            streaming_thought: None,
            streaming_text: None,
            reply: None,
            next_message: 1,
            next_call_order: 1,
            calls: HashMap::new(),
            emitted_calls: HashSet::new(),
            denied_calls: HashSet::new(),
            listed_tasks: Vec::new(),
            ended_tasks: Vec::new(),
            waiting: HashMap::new(),
        }
    }

    pub(super) fn in_turn(&self) -> bool {
        !self.prompts.is_empty() || self.ending != Ending::Not
    }

    /// Adds what a prompt of the turn took to what its earlier ones did, keeping either where the other said nothing.
    pub(super) fn add_tokens(&mut self, tokens: Option<TokenUsage>) {
        self.earlier_tokens = match (self.earlier_tokens.take(), tokens) {
            (Some(earlier), Some(tokens)) => Some(earlier + tokens),
            (earlier, tokens) => earlier.or(tokens),
        };
    }

    pub(super) fn has_prompt_rpc(&self, rpc_id: u64) -> bool {
        self.prompts.iter().any(|entry| entry.rpc_id == rpc_id)
    }

    pub(super) fn next_message_id(&mut self, prefix: &str) -> String {
        let id = self.next_message;
        self.next_message += 1;
        format!("agy-{prefix}-{id}")
    }

    pub(super) fn next_order(&mut self) -> u64 {
        let order = self.next_call_order;
        self.next_call_order += 1;
        order
    }

    /// Marks an in-flight wakeup drain to stay open through a model step because `agy` asked for interaction.
    pub(super) fn note_wakeup_interaction(&mut self) {
        if let Some(InFlightPrompt { origin: PromptOrigin::WakeupDrain { keep_open, .. }, .. }) =
            self.prompts.front_mut()
        {
            *keep_open = true;
        }
    }

    /// Starts an `OwnTurn` once for the active wakeup drain, taking any background tasks that ended beforehand.
    pub(super) fn begin_own_turn(&mut self) -> Option<OwnTurn> {
        let Some(InFlightPrompt { origin: PromptOrigin::WakeupDrain { started_own_turn, .. }, .. }) =
            self.prompts.front_mut()
        else {
            return None;
        };
        if *started_own_turn {
            return None;
        }
        *started_own_turn = true;
        let ended = std::mem::take(&mut self.ended_tasks);
        Some(if ended.is_empty() { OwnTurn::Unknown } else { OwnTurn::TaskEnded { ended } })
    }

    /// Takes any background tasks that ended before a failed drain prompt could start its `OwnTurn`.
    pub(super) fn take_ended_own_turn(&mut self) -> Option<OwnTurn> {
        let ended = std::mem::take(&mut self.ended_tasks);
        (!ended.is_empty()).then_some(OwnTurn::TaskEnded { ended })
    }

    /// Lists every surviving `Call::Command` in start order when a turn settles, returning the list if it grew.
    pub(super) fn list_surviving_commands(&mut self) -> Option<Vec<AgentTask>> {
        let mut pending: Vec<(u64, AgentTask)> = self
            .calls
            .iter()
            .filter(|(id, open)| {
                open.outlives_turn && !self.listed_tasks.iter().any(|task| task.id.as_str() == id.as_str())
            })
            .filter_map(|(id, open)| {
                let Call::Command(cmd) = &open.call else { return None };
                let description =
                    cmd.description.clone().or_else(|| (!cmd.command.is_empty()).then(|| cmd.command.clone()));
                Some((
                    open.order,
                    AgentTask {
                        id: TaskId::from(id.as_str()),
                        description,
                        kind: TaskKind::Command,
                        call: Some(id.clone()),
                    },
                ))
            })
            .collect();
        if pending.is_empty() {
            return None;
        }
        pending.sort_by_key(|(order, _)| *order);
        self.listed_tasks.extend(pending.into_iter().map(|(_, task)| task));
        Some(self.listed_tasks.clone())
    }

    /// Removes `id` from `listed_tasks` when its background command finishes, recording how it ended for `OwnTurn`.
    pub(super) fn finish_listed_task(
        &mut self,
        id: &ToolUseId,
        outcome: TaskOutcome,
    ) -> Option<(Vec<AgentTask>, Vec<TaskId>)> {
        let pos = self.listed_tasks.iter().position(|task| task.id.as_str() == id.as_str())?;
        let task = self.listed_tasks.remove(pos);
        let stopped = if outcome == TaskOutcome::Stopped { vec![task.id.clone()] } else { Vec::new() };
        self.ended_tasks.push(EndedTask { task, outcome, wait: None });
        Some((self.listed_tasks.clone(), stopped))
    }

    /// Consumes one `user_message_chunk` for the earliest prompt with non-empty `"text"` blocks left.
    pub(super) fn accept_user_chunk(&mut self) -> UserChunk {
        let Some(idx) = self.prompts.iter().position(|entry| entry.user_chunks_left > 0) else {
            return UserChunk::Ignore;
        };
        let has_later = idx + 1 < self.prompts.len();
        let has_waiting = !self.waiting.is_empty();
        let session_id = self.session_id.clone();
        let entry = &mut self.prompts[idx];
        entry.user_chunks_left -= 1;
        if entry.taken {
            return UserChunk::Ignore;
        }
        entry.taken = true;
        match &mut entry.origin {
            PromptOrigin::Prompt(seq) => UserChunk::FirstOfPrompt(*seq),
            PromptOrigin::WakeupDrain { keep_open, self_cancelled, .. } => {
                if *keep_open || has_later || has_waiting {
                    UserChunk::ContinueDrain
                } else if let Some(session_id) = session_id {
                    *self_cancelled = true;
                    UserChunk::CancelDrain { session_id }
                } else {
                    UserChunk::ContinueDrain
                }
            }
        }
    }
}
