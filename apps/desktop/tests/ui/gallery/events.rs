//! Building the events the screens' chats are made of, as the hub would record them from Claude.

#![cfg(test)]

use protocol::event::{ContentBlock, EventBody, FileChange, TurnOutcome};
use protocol::model::{MessageId, ToolUseId};

pub(crate) fn user_prompt(text: &str) -> EventBody {
    EventBody::UserPrompt { text: text.to_string(), client_msg_id: None, attachments: Vec::new(), tracked: false }
}

/// Something the agent wrote, as its own message, the way Claude sends each block.
pub(crate) fn text(message: &str, text: &str) -> EventBody {
    from_the_agent(message, ContentBlock::Text { text: text.to_string() })
}

pub(crate) fn tool_use(message: &str, id: &str, name: &str, input: serde_json::Value) -> EventBody {
    from_the_agent(message, ContentBlock::ToolUse { id: id.to_string(), name: name.to_string(), input })
}

/// What the agent thought, as its own message.
pub(crate) fn thought(message: &str, text: &str) -> EventBody {
    from_the_agent(message, ContentBlock::Thinking { text: text.to_string() })
}

fn from_the_agent(message: &str, block: ContentBlock) -> EventBody {
    EventBody::Assistant {
        message_id: Some(MessageId::from(message)),
        parent_tool_use_id: None,
        model: Some("claude-opus".to_string()),
        blocks: vec![block],
        first_index: None,
    }
}

pub(crate) fn tool_result(id: &str, text: &str) -> EventBody {
    tool_answer(id, text, false, None)
}

/// A result that says the call failed.
pub(crate) fn tool_failed(id: &str, text: &str) -> EventBody {
    tool_answer(id, text, true, None)
}

/// A result that says what the call changed in a file, as Claude's edits and writes do.
pub(crate) fn tool_changed(id: &str, text: &str, change: FileChange) -> EventBody {
    tool_answer(id, text, false, Some(change))
}

fn tool_answer(id: &str, text: &str, is_error: bool, change: Option<FileChange>) -> EventBody {
    EventBody::AgentUser {
        parent_tool_use_id: None,
        blocks: vec![ContentBlock::ToolResult {
            tool_use_id: ToolUseId::from(id),
            is_error,
            text: text.to_string(),
            image_count: 0,
            change,
        }],
    }
}

/// The end of a turn that went well.
pub(crate) fn finished_turn(seconds: u64, cost_usd: f64) -> EventBody {
    EventBody::TurnEnded {
        outcome: TurnOutcome::Success,
        result: None,
        errors: Vec::new(),
        duration_ms: Some(seconds * 1_000),
        cost_usd: Some(cost_usd),
        num_turns: None,
    }
}

/// `event` as the subagent started by the call `parent` sends it.
pub(crate) fn within(parent: &str, mut event: EventBody) -> EventBody {
    if let EventBody::Assistant { parent_tool_use_id, .. } | EventBody::AgentUser { parent_tool_use_id, .. } =
        &mut event
    {
        *parent_tool_use_id = Some(ToolUseId::from(parent));
    }
    event
}
