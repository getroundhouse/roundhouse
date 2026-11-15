use std::collections::{HashMap, HashSet, VecDeque};

use protocol::capability::{calls::Call, settings};
use protocol::event::TokenUsage;
use protocol::model::{AgentSessionId, ApprovalId, ModelName, PermissionMode, Seq, ToolUseId};
use serde_json::Value;

use crate::acp::{ConfigOption, FieldSpec, PermissionRequest};

pub(super) struct Conversation {
    pub(super) session_id: Option<AgentSessionId>,
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
    /// Open calls emitted as `ContentBlock::Call` and not yet settled by `CallResult` or `ToolCallDropped`.
    pub(super) calls: HashMap<ToolUseId, Call>,
    /// Every call ID already emitted as `ContentBlock::Call` in this run, so repeated `tool_call` / `tool_call_update`
    /// notifications for the same tool call never emit a second `Call` block.
    pub(super) emitted_calls: HashSet<ToolUseId>,
    pub(super) denied_calls: HashSet<ToolUseId>,
    pub(super) waiting: HashMap<ApprovalId, Waiting>,
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
    pub(super) seq: Option<Seq>,
    pub(super) taken: bool,
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
            calls: HashMap::new(),
            emitted_calls: HashSet::new(),
            denied_calls: HashSet::new(),
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

    /// Marks the active front prompt as taken once `agy` begins acting on it, returning its recorded `Seq` at most once.
    pub(super) fn take_front_prompt(&mut self) -> Option<Seq> {
        let front = self.prompts.front_mut()?;
        if front.taken {
            return None;
        }
        front.taken = true;
        front.seq
    }
}
