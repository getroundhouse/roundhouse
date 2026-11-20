use crate::event::{ContentBlock, EventBody};
use crate::model::{MessageId, RunId, Seq, SpokenThrough, SubagentId};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Text,
    Thinking,
    ToolUse,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StreamDelta {
    /// The launch of the agent that wrote it. The hub sets it, and only passes on the current run's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    pub message_id: MessageId,
    /// The subagent writing it, or `None` for the agent itself. Sent as `parent_tool_use_id`, its name from when a
    /// subagent was named by the call that started it.
    #[serde(default, rename = "parent_tool_use_id")]
    pub subagent: Option<SubagentId>,
    pub change: DeltaChange,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeltaChange {
    BlockStart {
        index: u32,
        kind: BlockKind,
        tool_name: Option<String>,
    },
    Text {
        index: u32,
        text: String,
    },
    Thinking {
        index: u32,
        text: String,
    },
    ToolInput {
        index: u32,
        json: String,
    },
    MessageStop,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Tail {
    /// More than one message can be open at once, because subagents stream alongside the main
    /// thread.
    pub messages: Vec<TailMessage>,
    /// The subagents working in the background, which carry on after the turn that started them ends, so its end
    /// leaves what they're writing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub in_background: Vec<InBackground>,
    /// The agent is compacting its conversation, as it last said, or has been asked to and hasn't said yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compacting: Option<Compacting>,
    /// The agent is trying a request to its model again, which failed, as it last said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrying: Option<Retrying>,
    /// What the agent is doing, said for it while it can't answer a prompt said out loud, as its adapter last said.
    /// Boxed, since it's there only in conversation mode, and every tail would otherwise carry room for its line, which
    /// a client's subscription moves by value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covering: Option<Box<Covering>>,
    /// How far the device that has the chat's conversation has read the agent's reply out loud, as it last said, while
    /// it's reading one. It's the device's rather than the agent's, so it outlasts the message being recorded, the turn
    /// and the agent, and it's in no run. Only the hub sets it, as a client sends it, so a worker's tail never holds it.
    /// Only the agent's own replies are read out, so a subagent's tail never holds it either.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaking: Option<SpokenThrough>,
}

/// A compaction under way, or asked for.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Compacting {
    /// The run compacting, so the end of an earlier run's turn can't say it's over. None for one only asked for so
    /// far, which the hub records itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    /// Where the request for it is recorded, until the agent says it has begun, so the request being dropped ends it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked: Option<Seq>,
    /// How much of the summary is written so far, in characters, when the agent says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_chars: Option<u64>,
}

/// A request of the agent's to its model that failed and that it's trying again, as its CLI says before each try. It's
/// over once the agent writes again, the request fails for good, or the turn or the agent ends.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retrying {
    /// The run retrying, so the end of an earlier run's turn can't say it's over. The hub sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    /// Which try comes next, from 1.
    pub attempt: u32,
    /// How many it tries before it gives up, when it says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<u32>,
    /// The HTTP status the last try failed with, when it got one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// How long it waits before the next try, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u64>,
}

/// What the agent is doing, in a sentence in its own voice, said for it while it can't answer the prompt said out loud
/// recorded at `prompt`, as [`crate::capability::output::Covering`] says. It's over once the agent writes words of its
/// own, which answer the prompt, or its turn or the agent ends. Thinking, a call or a subagent's words don't end it. A
/// client tells one line from the next by `prompt`, so a tail sent again whole doesn't say it twice.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Covering {
    /// The run it's said for, so the end of an earlier run's turn can't say it's over. The hub sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    pub prompt: Seq,
    pub line: String,
}

/// A subagent working in the background, and the run that started it, whose end it goes with.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct InBackground {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    /// Sent as `call`, its name from when a subagent was named by the call that started it.
    #[serde(rename = "call")]
    pub subagent: SubagentId,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TailMessage {
    /// The run writing it, so the end of an earlier run's turn can't clear it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    pub message_id: MessageId,
    /// The subagent writing it, or `None` for the agent itself, sent as [`StreamDelta`]'s is.
    #[serde(rename = "parent_tool_use_id")]
    pub subagent: Option<SubagentId>,
    pub blocks: Vec<TailBlock>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TailBlock {
    pub index: u32,
    pub kind: BlockKind,
    pub tool_name: Option<String>,
    /// Text, thinking text, or the partial JSON of a tool's input.
    pub text: String,
}

impl Tail {
    pub fn apply(&mut self, delta: &StreamDelta) {
        if delta.subagent.is_none() {
            self.retried(delta.run);
            if matches!(delta.change, DeltaChange::Text { .. } | DeltaChange::BlockStart { kind: BlockKind::Text, .. })
            {
                self.covered(delta.run);
            }
        }
        if matches!(delta.change, DeltaChange::MessageStop) {
            self.messages.retain(|m| m.message_id != delta.message_id);
            return;
        }
        let position = self.messages.iter().position(|m| m.message_id == delta.message_id).unwrap_or_else(|| {
            self.messages.push(TailMessage {
                run: delta.run,
                message_id: delta.message_id.clone(),
                subagent: delta.subagent.clone(),
                blocks: Vec::new(),
            });
            self.messages.len() - 1
        });
        let message = &mut self.messages[position];
        match &delta.change {
            DeltaChange::BlockStart { index, kind, tool_name } => {
                let block = block_mut(message, *index, *kind);
                block.kind = *kind;
                block.tool_name.clone_from(tool_name);
            }
            DeltaChange::Text { index, text } => block_mut(message, *index, BlockKind::Text).text.push_str(text),
            DeltaChange::Thinking { index, text } => {
                block_mut(message, *index, BlockKind::Thinking).text.push_str(text);
            }
            DeltaChange::ToolInput { index, json } => {
                block_mut(message, *index, BlockKind::ToolUse).text.push_str(json);
            }
            DeltaChange::MessageStop | DeltaChange::Unknown => {}
        }
    }

    /// Updates the tail for a durable event from `run`, recorded at `seq`. Claude sends a finished block as an
    /// `Assistant` event before starting the next block, so everything open for that message is now
    /// in the log. The end of a turn clears what that run or an earlier one left open, but for what a
    /// subagent in the background is writing, since it carries on. The end of the agent clears that
    /// too, and a subagent's end clears what it was writing. Neither end clears what a later run is
    /// writing: a replaced agent's last events can arrive after its replacement has started. An event
    /// with no run, which the hub recorded itself, is taken to come after every run. A compaction starts when it's
    /// asked for, so the wait for an agent to start isn't a bare wait, or when the agent says it has. It's over when it
    /// says so, when its request is dropped, when its turn or its agent ends, or when the agent writes again, which it
    /// can't while compacting.
    pub fn on_event(&mut self, seq: Seq, run: Option<RunId>, body: &EventBody) {
        self.heard(Some(seq), run, body);
    }

    /// What [`Tail::on_event`] does, for an event recorded at `seq`, or one no hub has recorded yet.
    pub(super) fn heard(&mut self, seq: Option<Seq>, run: Option<RunId>, body: &EventBody) {
        match body {
            EventBody::Assistant { message_id, subagent, blocks, .. } => {
                if let Some(id) = message_id {
                    self.messages.retain(|m| m.message_id != *id);
                }
                if subagent.is_none() {
                    self.compacted(run);
                    self.retried(run);
                    if blocks.iter().any(|block| matches!(block, ContentBlock::Text { .. })) {
                        self.covered(run);
                    }
                }
            }
            EventBody::RequestFailed { subagent: None, .. } => self.retried(run),
            EventBody::TurnEnded { .. } => self.turn_ended(run),
            EventBody::AgentExited { .. } => self.agent_exited(run),
            EventBody::SubagentStarted { in_background: true, .. } => {
                if let Some(subagent) = body.subagent_started() {
                    self.in_background(run, subagent);
                }
            }
            EventBody::SubagentEnded { subagent, .. } => {
                self.messages.retain(|m| m.subagent.as_ref() != Some(subagent));
                self.in_background.retain(|b| b.subagent != *subagent);
            }
            EventBody::CompactionAsked { .. } => {
                self.compacting = Some(Compacting { run, asked: seq, written_chars: None });
            }
            EventBody::AgentCompacting { written_chars } => {
                self.compacting = Some(Compacting { run, asked: None, written_chars: *written_chars });
            }
            EventBody::AgentCompacted { .. } => self.compacted(run),
            EventBody::PromptDropped { prompt } => {
                self.compacting = self.compacting.take().filter(|compacting| compacting.asked != Some(*prompt));
            }
            EventBody::UserPrompt { .. }
            | EventBody::PromptTaken { .. }
            | EventBody::AgentStarted { .. }
            | EventBody::AgentInit { .. }
            | EventBody::AgentState { .. }
            | EventBody::AgentTasks { .. }
            | EventBody::OwnTurn(_)
            | EventBody::AgentLimits { .. }
            | EventBody::AgentUser { .. }
            | EventBody::ToolCallDropped { .. }
            | EventBody::ApprovalRequested { .. }
            | EventBody::ApprovalResolved { .. }
            | EventBody::Notice { .. }
            | EventBody::RequestFailed { subagent: Some(_), .. }
            | EventBody::AgentMode { .. }
            | EventBody::SubagentStarted { in_background: false, .. }
            | EventBody::SubagentContext { .. }
            | EventBody::TaskChanged { .. }
            | EventBody::AgentOther { .. }
            | EventBody::CacheExpired { .. }
            | EventBody::ConversationStarted { .. }
            | EventBody::VoiceCut { .. }
            | EventBody::Unknown => {}
        }
    }

    /// The turn of `run` ended, which ends what it was writing, but for what a subagent in the background is writing,
    /// since it carries on.
    fn turn_ended(&mut self, run: Option<RunId>) {
        let in_background = &self.in_background;
        let carries_on = |subagent: &SubagentId| in_background.iter().any(|b| b.subagent == *subagent);
        self.messages.retain(|m| later(run, m.run) || m.subagent.as_ref().is_some_and(carries_on));
        self.compacted(run);
        self.retried(run);
        self.covered(run);
    }

    /// The agent of `run` exited, which ends everything it and its subagents were writing.
    fn agent_exited(&mut self, run: Option<RunId>) {
        self.messages.retain(|m| later(run, m.run));
        self.in_background.retain(|b| later(run, b.run));
        self.compacted(run);
        self.retried(run);
        self.covered(run);
    }

    /// `subagent`, which `run` started, works in the background, so the end of a turn leaves what it's writing.
    fn in_background(&mut self, run: Option<RunId>, subagent: SubagentId) {
        if !self.in_background.iter().any(|b| b.subagent == subagent) {
            self.in_background.push(InBackground { run, subagent });
        }
    }

    /// `run`'s compaction is over, or an earlier run's.
    pub(super) fn compacted(&mut self, run: Option<RunId>) {
        self.compacting = self.compacting.take().filter(|compacting| later(run, compacting.run));
    }

    /// The agent of `run` is trying a failed request again, as `retrying` says, with the run set.
    pub fn retrying(&mut self, run: RunId, retrying: Retrying) {
        self.retrying = Some(Retrying { run: Some(run), ..retrying });
    }

    /// What the agent of `run` is doing is said for it, as `covering` says, with the run set.
    pub fn covering(&mut self, run: RunId, covering: Covering) {
        self.covering = Some(Box::new(Covering { run: Some(run), ..covering }));
    }

    /// `run`'s covering is over, or an earlier run's.
    fn covered(&mut self, run: Option<RunId>) {
        self.covering = self.covering.take().filter(|covering| later(run, covering.run));
    }

    /// `run`'s retrying is over, or an earlier run's.
    fn retried(&mut self, run: Option<RunId>) {
        self.retrying = self.retrying.take().filter(|retrying| later(run, retrying.run));
    }

    /// Whether nothing is being written. A reply being read out is written already.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty() && self.compacting.is_none() && self.retrying.is_none() && self.covering.is_none()
    }

    /// Whether it holds nothing of any run, not even a subagent in the background, which [`Tail::is_empty`] leaves out
    /// since one writes nothing until it does.
    pub fn holds_nothing(&self) -> bool {
        // How far a reply has been read out is in no run.
        let Tail { messages, in_background, compacting, retrying, covering, speaking: _ } = self;
        let unsaid = compacting.is_none() && retrying.is_none() && covering.is_none();
        messages.is_empty() && in_background.is_empty() && unsaid
    }

    /// Updates the tail for an event from `run` that no hub has recorded yet, as its worker sends it on, so it has
    /// no seq. Only an event the hub records itself, a compaction being asked for, needs its place in the log, and no
    /// agent sends one.
    pub fn on_agent_event(&mut self, run: RunId, body: &EventBody) {
        self.heard(None, Some(run), body);
    }
}

/// Whether what `by` writes comes from a run after `ended`. Nothing comes after an event with no run.
fn later(ended: Option<RunId>, by: Option<RunId>) -> bool {
    ended.is_some_and(|ended| by.is_some_and(|by| by > ended))
}

fn block_mut(message: &mut TailMessage, index: u32, kind: BlockKind) -> &mut TailBlock {
    let position = message.blocks.iter().position(|b| b.index == index).unwrap_or_else(|| {
        message.blocks.push(TailBlock { index, kind, tool_name: None, text: String::new() });
        message.blocks.len() - 1
    });
    &mut message.blocks[position]
}
