//! The streaming tail: text the agent is still writing, which isn't in the log yet.
//!
//! Deltas aren't stored, because the finished message arrives as a durable event and replaces
//! them. A client that attaches mid-message gets a [`Tail`] snapshot and then deltas, so it sees
//! the partial text straight away. The hub and clients apply the same rules through this type, so
//! they can't disagree about what the tail contains.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::event::EventBody;
use crate::model::{MessageId, RunId, Seq, ToolUseId};

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
    #[serde(default)]
    pub parent_tool_use_id: Option<ToolUseId>,
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

/// A subagent working in the background, and the run that started it, whose end it goes with.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct InBackground {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    pub call: ToolUseId,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TailMessage {
    /// The run writing it, so the end of an earlier run's turn can't clear it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    pub message_id: MessageId,
    pub parent_tool_use_id: Option<ToolUseId>,
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
        if matches!(delta.change, DeltaChange::MessageStop) {
            self.messages.retain(|m| m.message_id != delta.message_id);
            return;
        }
        let position = self.messages.iter().position(|m| m.message_id == delta.message_id).unwrap_or_else(|| {
            self.messages.push(TailMessage {
                run: delta.run,
                message_id: delta.message_id.clone(),
                parent_tool_use_id: delta.parent_tool_use_id.clone(),
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
        match body {
            EventBody::Assistant { message_id, parent_tool_use_id, .. } => {
                if let Some(id) = message_id {
                    self.messages.retain(|m| m.message_id != *id);
                }
                if parent_tool_use_id.is_none() {
                    self.compacted(run);
                }
            }
            EventBody::TurnEnded { .. } => {
                let in_background = &self.in_background;
                let carries_on = |call: &ToolUseId| in_background.iter().any(|b| b.call == *call);
                self.messages.retain(|m| later(run, m.run) || m.parent_tool_use_id.as_ref().is_some_and(carries_on));
                self.compacted(run);
            }
            EventBody::AgentExited { .. } => {
                self.messages.retain(|m| later(run, m.run));
                self.in_background.retain(|b| later(run, b.run));
                self.compacted(run);
            }
            EventBody::SubagentStarted { call, in_background: true, .. } => {
                if !self.in_background.iter().any(|b| b.call == *call) {
                    self.in_background.push(InBackground { run, call: call.clone() });
                }
            }
            EventBody::SubagentEnded { call, .. } => {
                self.messages.retain(|m| m.parent_tool_use_id.as_ref() != Some(call));
                self.in_background.retain(|b| b.call != *call);
            }
            EventBody::CompactionAsked { .. } => {
                self.compacting = Some(Compacting { run, asked: Some(seq), written_chars: None });
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
            | EventBody::AgentLimits { .. }
            | EventBody::AgentUser { .. }
            | EventBody::ToolCallDropped { .. }
            | EventBody::ApprovalRequested { .. }
            | EventBody::ApprovalResolved { .. }
            | EventBody::Notice { .. }
            | EventBody::RequestFailed { .. }
            | EventBody::AgentMode { .. }
            | EventBody::SubagentStarted { in_background: false, .. }
            | EventBody::SubagentContext { .. }
            | EventBody::AgentOther { .. }
            | EventBody::CacheExpired { .. }
            | EventBody::Unknown => {}
        }
    }

    /// `run`'s compaction is over, or an earlier run's.
    fn compacted(&mut self, run: Option<RunId>) {
        self.compacting = self.compacting.take().filter(|compacting| later(run, compacting.run));
    }

    /// Whether nothing is being written.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty() && self.compacting.is_none()
    }

    /// Whether it holds nothing of any run, not even a subagent in the background, which [`Tail::is_empty`] leaves out
    /// since one writes nothing until it does.
    pub fn holds_nothing(&self) -> bool {
        let Tail { messages, in_background, compacting } = self;
        messages.is_empty() && in_background.is_empty() && compacting.is_none()
    }

    /// What the agent itself is writing, or the subagent `subagent` started. It holds no other's messages for the
    /// end of a turn to leave, so it doesn't say which subagents are in the background.
    #[must_use]
    pub fn of(&self, subagent: Option<&ToolUseId>) -> Tail {
        let messages = self.messages.iter().filter(|message| message.parent_tool_use_id.as_ref() == subagent);
        // Only the agent compacts its conversation, so a subagent's tail never says it's compacting.
        let compacting = self.compacting.clone().filter(|_| subagent.is_none());
        Tail { messages: messages.cloned().collect(), in_background: Vec::new(), compacting }
    }

    /// Updates the tail for an event from `run` that no hub has recorded yet, as its worker sends it on. Only an event
    /// the hub records itself, a compaction being asked for, needs its place in the log, and no agent sends one.
    pub fn on_agent_event(&mut self, run: RunId, body: &EventBody) {
        self.on_event(NOT_RECORDED, Some(run), body);
    }

    /// Every run it holds something of, oldest first.
    #[must_use]
    pub fn runs(&self) -> Vec<RunId> {
        let messages = self.messages.iter().map(|message| message.run);
        let in_background = self.in_background.iter().map(|background| background.run);
        let compacting = self.compacting.iter().map(|compacting| compacting.run);
        let runs: BTreeSet<RunId> = messages.chain(in_background).chain(compacting).flatten().collect();
        runs.into_iter().collect()
    }

    /// What `run` is writing, the subagents it runs in the background, and its compaction, which a worker sends a hub
    /// that connects to it.
    #[must_use]
    pub fn of_run(&self, run: RunId) -> Tail {
        let of = |by: Option<RunId>| by == Some(run);
        Tail {
            messages: self.messages.iter().filter(|message| of(message.run)).cloned().collect(),
            in_background: self.in_background.iter().filter(|background| of(background.run)).cloned().collect(),
            compacting: self.compacting.clone().filter(|compacting| of(compacting.run)),
        }
    }

    /// Takes what `run` is writing from `written`, its worker's tail of it, in place of what this holds of it, as a hub
    /// does from a worker it connects to again, since the deltas that wrote it went to the hub before, or to none.
    /// Everything in `written` is taken as `run`'s. A compaction only asked for so far is the hub's own, so it stays
    /// unless `written` says `run` is compacting.
    pub fn replace_run(&mut self, run: RunId, written: Tail) {
        let of = |by: Option<RunId>| by == Some(run);
        self.messages.retain(|message| !of(message.run));
        self.messages.extend(written.messages.into_iter().map(|message| TailMessage { run: Some(run), ..message }));
        self.in_background.retain(|background| !of(background.run));
        let in_background = written.in_background.into_iter();
        self.in_background.extend(in_background.map(|background| InBackground { run: Some(run), ..background }));
        self.compacting = match written.compacting {
            Some(compacting) => Some(Compacting { run: Some(run), ..compacting }),
            None => self.compacting.take().filter(|compacting| !of(compacting.run)),
        };
    }
}

/// Where an event no hub has recorded yet is in the log, which is nowhere, since the first event is 1.
const NOT_RECORDED: Seq = 0;

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

#[cfg(test)]
mod carrying_on;
#[cfg(test)]
mod compacting;
#[cfg(test)]
mod laws;

#[cfg(test)]
mod tests;
