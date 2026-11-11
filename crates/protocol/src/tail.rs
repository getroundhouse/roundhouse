//! The streaming tail: text the agent is still writing, which isn't in the log yet.
//!
//! Deltas aren't stored, because the finished message arrives as a durable event and replaces
//! them. A client that attaches mid-message gets a [`Tail`] snapshot and then deltas, so it sees
//! the partial text straight away. The hub and clients apply the same rules through this type, so
//! they can't disagree about what the tail contains.

use serde::{Deserialize, Serialize};

use crate::event::EventBody;
use crate::model::{MessageId, RunId, ToolUseId};

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

    /// Updates the tail for a durable event from `run`. Claude sends a finished block as an
    /// `Assistant` event before starting the next block, so everything open for that message is now
    /// in the log. The end of a turn clears what that run or an earlier one left open, but for what a
    /// subagent in the background is writing, since it carries on. The end of the agent clears that
    /// too, and a subagent's end clears what it was writing. Neither end clears what a later run is
    /// writing: a replaced agent's last events can arrive after its replacement has started. An event
    /// with no run, which the hub recorded itself, is taken to come after every run.
    pub fn on_event(&mut self, run: Option<RunId>, body: &EventBody) {
        match body {
            EventBody::Assistant { message_id: Some(id), .. } => {
                self.messages.retain(|m| m.message_id != *id);
            }
            EventBody::TurnEnded { .. } => {
                let in_background = &self.in_background;
                let carries_on = |call: &ToolUseId| in_background.iter().any(|b| b.call == *call);
                self.messages.retain(|m| later(run, m.run) || m.parent_tool_use_id.as_ref().is_some_and(carries_on));
            }
            EventBody::AgentExited { .. } => {
                self.messages.retain(|m| later(run, m.run));
                self.in_background.retain(|b| later(run, b.run));
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
            EventBody::Assistant { message_id: None, .. }
            | EventBody::UserPrompt { .. }
            | EventBody::PromptTaken { .. }
            | EventBody::PromptDropped { .. }
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
            | EventBody::AgentMode { .. }
            | EventBody::SubagentStarted { in_background: false, .. }
            | EventBody::AgentOther { .. }
            | EventBody::Unknown => {}
        }
    }

    /// Whether nothing is being written.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// What the agent itself is writing, or the subagent `subagent` started. It holds no other's messages for the
    /// end of a turn to leave, so it doesn't say which subagents are in the background.
    #[must_use]
    pub fn of(&self, subagent: Option<&ToolUseId>) -> Tail {
        let messages = self.messages.iter().filter(|message| message.parent_tool_use_id.as_ref() == subagent);
        Tail { messages: messages.cloned().collect(), in_background: Vec::new() }
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

#[cfg(test)]
mod laws;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{ExitReason, SubagentOutcome};

    fn delta(change: DeltaChange) -> StreamDelta {
        StreamDelta { run: None, message_id: "m1".into(), parent_tool_use_id: None, change }
    }

    fn assistant(id: &str) -> EventBody {
        EventBody::Assistant {
            message_id: Some(id.into()),
            parent_tool_use_id: None,
            model: None,
            blocks: vec![],
            first_index: None,
        }
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
        tail.on_event(None, &assistant("m1"));
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
            run: None,
            message_id: "sub".into(),
            parent_tool_use_id: Some("toolu_1".into()),
            change: DeltaChange::Text { index: 0, text: "sub".into() },
        });
        assert_eq!(tail.messages.len(), 2);
        tail.on_event(None, &assistant("sub"));
        assert_eq!(tail.messages.len(), 1);
        assert_eq!(tail.messages[0].message_id, "m1".into());
    }

    #[test]
    fn the_tail_of_the_agent_or_a_subagent_is_only_what_that_one_writes() {
        let mut tail = Tail::default();
        tail.apply(&delta(DeltaChange::Text { index: 0, text: "own".into() }));
        for (message, call) in [("a1", "toolu_a"), ("b1", "toolu_b"), ("a2", "toolu_a")] {
            let change = DeltaChange::Text { index: 0, text: message.into() };
            tail.apply(&StreamDelta {
                run: None,
                message_id: message.into(),
                parent_tool_use_id: Some(call.into()),
                change,
            });
        }
        let ids = |tail: Tail| tail.messages.into_iter().map(|m| m.message_id).collect::<Vec<_>>();
        assert_eq!(ids(tail.of(None)), vec!["m1".into()]);
        assert_eq!(ids(tail.of(Some(&"toolu_a".into()))), vec![MessageId::from("a1"), "a2".into()]);
        assert_eq!(ids(tail.of(Some(&"toolu_b".into()))), vec![MessageId::from("b1")]);
        assert!(tail.of(Some(&"toolu_c".into())).is_empty());
    }

    fn turn_ended() -> EventBody {
        EventBody::TurnEnded {
            outcome: crate::event::TurnOutcome::Cancelled,
            result: None,
            errors: vec![],
            duration_ms: None,
            cost_usd: None,
            num_turns: None,
            started_at_ms: None,
            models: vec![],
            tokens: None,
        }
    }

    fn started(call: &str, in_background: bool) -> EventBody {
        EventBody::SubagentStarted { call: call.into(), kind: None, description: None, prompt: None, in_background }
    }

    /// Text that `call`'s subagent writes, in its message `message`.
    fn writes(tail: &mut Tail, call: &str, message: &str, text: &str) {
        let change = DeltaChange::Text { index: 0, text: text.into() };
        let run = Some(RunId::new(1));
        tail.apply(&StreamDelta { run, message_id: message.into(), parent_tool_use_id: Some(call.into()), change });
    }

    fn text(tail: &Tail) -> String {
        tail.messages.iter().flat_map(|m| &m.blocks).map(|b| b.text.as_str()).collect()
    }

    #[test]
    fn the_end_of_a_turn_clears_what_the_agent_was_writing() {
        let mut tail = Tail::default();
        tail.apply(&delta(DeltaChange::Text { index: 0, text: "cut off".into() }));
        tail.on_event(None, &turn_ended());
        assert!(tail.is_empty());
    }

    #[test]
    fn the_end_of_a_turn_leaves_what_a_subagent_in_the_background_writes_and_clears_one_in_the_foreground() {
        let (mut tail, run) = (Tail::default(), Some(RunId::new(1)));
        tail.on_event(run, &started("toolu_bg", true));
        tail.on_event(run, &started("toolu_fg", false));
        writes(&mut tail, "toolu_bg", "bg", "There's ");
        writes(&mut tail, "toolu_fg", "fg", "Half");

        tail.on_event(run, &turn_ended());
        writes(&mut tail, "toolu_bg", "bg", "a README ");

        assert_eq!(text(&tail.of(Some(&"toolu_bg".into()))), "There's a README ");
        assert!(tail.of(Some(&"toolu_fg".into())).is_empty());
    }

    #[test]
    fn a_subagents_end_clears_what_it_was_writing_and_the_agents_end_clears_the_rest() {
        let (mut tail, run) = (Tail::default(), Some(RunId::new(1)));
        tail.on_event(run, &started("toolu_a", true));
        tail.on_event(run, &started("toolu_b", true));
        writes(&mut tail, "toolu_a", "a", "Cut off");
        writes(&mut tail, "toolu_b", "b", "Going on");

        tail.on_event(
            run,
            &EventBody::SubagentEnded { call: "toolu_a".into(), outcome: SubagentOutcome::Stopped, report: None },
        );
        assert_eq!(text(&tail), "Going on");
        writes(&mut tail, "toolu_a", "a2", "Late");
        tail.on_event(run, &turn_ended());
        assert_eq!(text(&tail), "Going on", "an ended subagent is no longer in the background");

        let exited =
            EventBody::AgentExited { reason: ExitReason::Requested, code: Some(0), signal: None, stderr_tail: vec![] };
        tail.on_event(run, &exited);
        assert_eq!(tail, Tail::default(), "nothing is left, nor which subagents were in the background");
    }
}
