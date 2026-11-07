//! The streaming tail: text the agent is still writing, which isn't in the log yet.
//!
//! Deltas aren't stored, because the finished message arrives as a durable event and replaces
//! them. A client that attaches mid-message gets a [`Tail`] snapshot and then deltas, so it sees
//! the partial text straight away. The hub and clients apply the same rules through this type, so
//! they can't disagree about what the tail contains.

use serde::{Deserialize, Serialize};

use crate::event::EventBody;

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
    pub message_id: String,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
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
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TailMessage {
    pub message_id: String,
    pub parent_tool_use_id: Option<String>,
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

    /// Updates the tail for a durable event. Claude sends a finished block as an `Assistant` event
    /// before starting the next block, so everything open for that message is now in the log.
    pub fn on_event(&mut self, body: &EventBody) {
        match body {
            EventBody::Assistant { message_id: Some(id), .. } => {
                self.messages.retain(|m| m.message_id != *id);
            }
            EventBody::TurnEnded { .. } | EventBody::AgentExited { .. } => self.messages.clear(),
            EventBody::Assistant { message_id: None, .. }
            | EventBody::UserPrompt { .. }
            | EventBody::AgentStarted { .. }
            | EventBody::AgentInit { .. }
            | EventBody::AgentState { .. }
            | EventBody::AgentUser { .. }
            | EventBody::ApprovalRequested { .. }
            | EventBody::ApprovalResolved { .. }
            | EventBody::Notice { .. }
            | EventBody::AgentMode { .. }
            | EventBody::AgentOther { .. }
            | EventBody::Unknown => {}
        }
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

fn block_mut(message: &mut TailMessage, index: u32, kind: BlockKind) -> &mut TailBlock {
    let position = message.blocks.iter().position(|b| b.index == index).unwrap_or_else(|| {
        message.blocks.push(TailBlock { index, kind, tool_name: None, text: String::new() });
        message.blocks.len() - 1
    });
    &mut message.blocks[position]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(change: DeltaChange) -> StreamDelta {
        StreamDelta { message_id: "m1".into(), parent_tool_use_id: None, change }
    }

    fn assistant(id: &str) -> EventBody {
        EventBody::Assistant { message_id: Some(id.into()), parent_tool_use_id: None, model: None, blocks: vec![] }
    }

    #[test]
    fn deltas_build_up_text_even_without_a_block_start() {
        let mut tail = Tail::default();
        tail.apply(&delta(DeltaChange::Text { index: 0, text: "hel".into() }));
        tail.apply(&delta(DeltaChange::Text { index: 0, text: "lo".into() }));
        assert_eq!(tail.messages[0].blocks[0].text, "hello");
        assert_eq!(tail.messages[0].blocks[0].kind, BlockKind::Text);
    }

    #[test]
    fn a_finished_block_leaves_the_tail_and_the_next_block_starts_clean() {
        let mut tail = Tail::default();
        tail.apply(&delta(DeltaChange::Text { index: 0, text: "first".into() }));
        tail.on_event(&assistant("m1"));
        assert!(tail.is_empty());
        tail.apply(&delta(DeltaChange::BlockStart {
            index: 1,
            kind: BlockKind::ToolUse,
            tool_name: Some("Bash".into()),
        }));
        tail.apply(&delta(DeltaChange::ToolInput { index: 1, json: "{\"c".into() }));
        let block = &tail.messages[0].blocks[0];
        assert_eq!((block.index, block.kind, block.text.as_str()), (1, BlockKind::ToolUse, "{\"c"));
    }

    #[test]
    fn messages_stay_separate_and_only_the_named_one_is_cleared() {
        let mut tail = Tail::default();
        tail.apply(&delta(DeltaChange::Text { index: 0, text: "main".into() }));
        tail.apply(&StreamDelta {
            message_id: "sub".into(),
            parent_tool_use_id: Some("toolu_1".into()),
            change: DeltaChange::Text { index: 0, text: "sub".into() },
        });
        assert_eq!(tail.messages.len(), 2);
        tail.on_event(&assistant("sub"));
        assert_eq!(tail.messages.len(), 1);
        assert_eq!(tail.messages[0].message_id, "m1");
    }

    #[test]
    fn the_end_of_a_turn_clears_everything() {
        let mut tail = Tail::default();
        tail.apply(&delta(DeltaChange::Text { index: 0, text: "cut off".into() }));
        tail.on_event(&EventBody::TurnEnded {
            outcome: crate::event::TurnOutcome::Cancelled,
            result: None,
            errors: vec![],
            duration_ms: None,
            cost_usd: None,
            num_turns: None,
        });
        assert!(tail.is_empty());
    }
}
