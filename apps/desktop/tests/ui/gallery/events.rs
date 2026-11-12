//! Building the events the screens' chats are made of, as the hub would record them from Claude.

#![cfg(test)]

use protocol::attachment::Attachment;
use protocol::event::{
    AgentTask, ContentBlock, EventBody, FileChange, LinkedImage, SubagentOutcome, TaskKind, TokenUsage, TurnOutcome,
};
use protocol::model::{MessageId, ModelName, Seq, ToolUseId};

pub(crate) fn user_prompt(text: &str) -> EventBody {
    EventBody::UserPrompt { text: text.to_string(), client_msg_id: None, attachments: Vec::new() }
}

/// The agent taking the prompt recorded at `prompt`, which is the first event of every chat here.
pub(crate) fn taken(prompt: Seq) -> EventBody {
    EventBody::PromptTaken { prompt }
}

/// Something the agent wrote, as its own message, the way Claude sends each block.
pub(crate) fn text(message: &str, text: &str) -> EventBody {
    text_pointing(message, text, Vec::new())
}

/// The same, with the images in the worktree it points to, which the hub kept.
pub(crate) fn text_pointing(message: &str, text: &str, images: Vec<LinkedImage>) -> EventBody {
    from_the_agent(message, ContentBlock::Text { text: text.to_string(), images })
}

pub(crate) fn tool_use(message: &str, id: &str, name: &str, input: serde_json::Value) -> EventBody {
    from_the_agent(message, ContentBlock::ToolUse { id: id.to_string(), name: name.to_string(), input })
}

/// What the agent thought, as its own message.
pub(crate) fn thought(message: &str, text: &str) -> EventBody {
    from_the_agent(message, ContentBlock::Thinking { text: text.to_string() })
}

/// What the advisor said. Its reply comes from the agent, in the call's message, since the API runs it.
pub(crate) fn advised(message: &str, id: &str, text: &str) -> EventBody {
    from_the_agent(message, answer(id, text, false, None))
}

/// The advisor's reply, encrypted for the model alone.
pub(crate) fn advised_encrypted(message: &str, id: &str) -> EventBody {
    let mut block = answer(id, "", false, None);
    if let ContentBlock::ToolResult { encrypted, .. } = &mut block {
        *encrypted = true;
    }
    from_the_agent(message, block)
}

/// Why asking the advisor failed.
pub(crate) fn advice_failed(message: &str, id: &str, text: &str) -> EventBody {
    from_the_agent(message, answer(id, text, true, None))
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

/// A result that's only images, as a read of one is.
pub(crate) fn tool_images(id: &str, images: Vec<Attachment>) -> EventBody {
    let mut event = tool_answer(id, "", false, None);
    if let EventBody::AgentUser { blocks, .. } = &mut event
        && let [ContentBlock::ToolResult { images: shown, .. }] = blocks.as_mut_slice()
    {
        *shown = images;
    }
    event
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
    EventBody::AgentUser { parent_tool_use_id: None, blocks: vec![answer(id, text, is_error, change)] }
}

fn answer(id: &str, text: &str, is_error: bool, change: Option<FileChange>) -> ContentBlock {
    let (tool_use_id, text, images) = (ToolUseId::from(id), text.to_string(), Vec::new());
    ContentBlock::ToolResult { tool_use_id, is_error, text, images, change, encrypted: false }
}

/// The end of a turn that went well. Opus answered, and read most of what it was sent from the cache, as a turn that
/// reads around a repository does.
pub(crate) fn finished_turn(seconds: u64, cost_usd: f64) -> EventBody {
    let tokens =
        TokenUsage { input_tokens: 6, output_tokens: 1_840, cache_read_tokens: 412_377, cache_write_tokens: 9_615 };
    EventBody::TurnEnded {
        outcome: TurnOutcome::Success,
        result: None,
        errors: Vec::new(),
        duration_ms: Some(seconds * 1_000),
        cost_usd: Some(cost_usd),
        num_turns: None,
        started_at_ms: None,
        models: vec![ModelName::named("claude-opus-5-5")],
        tokens: Some(tokens),
        only_compacted: false,
    }
}

/// A turn that finished saying `reply`, as the CLI ends one, so its copy button has something to copy.
pub(crate) fn replied(reply: &str, seconds: u64, cost_usd: f64) -> EventBody {
    let mut ended = finished_turn(seconds, cost_usd);
    if let EventBody::TurnEnded { result, .. } = &mut ended {
        *result = Some(reply.to_string());
    }
    ended
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

/// The agent's word that a subagent started in the background, as the hub records it from the CLI's.
pub(crate) fn subagent_started(call: &str, kind: &str, description: &str, prompt: &str) -> EventBody {
    EventBody::SubagentStarted {
        call: call.into(),
        kind: Some(kind.into()),
        description: Some(description.into()),
        prompt: Some(prompt.into()),
        in_background: true,
    }
}

/// The subagents working in the background, each by the call that started it and what it's for, as the hub records
/// the list once the agent has said which call started each.
pub(crate) fn subagents_listed(subagents: &[(&str, &str)]) -> EventBody {
    let task = |(call, description): &(&str, &str)| AgentTask {
        id: format!("task-{call}").into(),
        description: Some(description.to_string()),
        kind: TaskKind::Subagent,
        call: Some(ToolUseId::from(*call)),
    };
    EventBody::AgentTasks { tasks: subagents.iter().map(task).collect(), stopped: Vec::new() }
}

/// The agent's word that a subagent finished, with its report.
pub(crate) fn subagent_ended(call: &str, report: &str) -> EventBody {
    EventBody::SubagentEnded { call: call.into(), outcome: SubagentOutcome::Finished, report: Some(report.into()) }
}
