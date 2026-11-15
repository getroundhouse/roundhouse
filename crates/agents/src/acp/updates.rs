use protocol::event::{TokenUsage, TurnOutcome};
use serde::Deserialize;
use serde_json::Value;

use super::ConfigOption;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SessionUpdate {
    UserMessageChunk { text: String },
    AgentMessageChunk { text: String },
    AgentThoughtChunk { text: String },
    ToolCall(ToolCallWire),
    ToolCallUpdate(ToolCallWire),
    CurrentModeUpdate { mode_id: String },
    ConfigOptionUpdate { config_options: Vec<ConfigOption> },
    UsageUpdate { used: u64, size: u64 },
    Other,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ToolCallWire {
    pub(crate) tool_call_id: String,
    pub(crate) status: Option<ToolStatusWire>,
    pub(crate) kind: Option<ToolKindWire>,
    pub(crate) title: Option<String>,
    /// Non-standard fields emitted by `agy acp` alongside ACP's standard `kind` and `rawInput`.
    pub(crate) name: Option<String>,
    pub(crate) args: Option<Value>,
    pub(crate) raw_input: Option<Value>,
    pub(crate) raw_output: Option<Value>,
    pub(crate) locations: Vec<String>,
    pub(crate) content_text: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolStatusWire {
    Pending,
    InProgress,
    Completed,
    Failed,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolKindWire {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PromptResponse {
    pub(crate) stop_reason: StopReason,
    pub(crate) tokens: Option<TokenUsage>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StopReason {
    EndTurn,
    Cancelled,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Other,
}

impl StopReason {
    pub(crate) fn outcome(self) -> TurnOutcome {
        match self {
            StopReason::EndTurn => TurnOutcome::Success,
            StopReason::Cancelled => TurnOutcome::Cancelled,
            StopReason::MaxTokens | StopReason::MaxTurnRequests | StopReason::Refusal | StopReason::Other => {
                TurnOutcome::Error
            }
        }
    }
}

#[derive(Deserialize)]
struct RawUsage {
    #[serde(default, rename = "inputTokens")]
    input: u64,
    #[serde(default, rename = "outputTokens")]
    output: u64,
    #[serde(default, rename = "cachedReadTokens")]
    cached_read: Option<u64>,
    #[serde(default, rename = "cachedWriteTokens")]
    cached_write: Option<u64>,
}

pub(crate) fn parse_prompt_response(result: &Value) -> PromptResponse {
    let stop_reason = match result.get("stopReason").and_then(Value::as_str) {
        Some("end_turn") => StopReason::EndTurn,
        Some("cancelled") => StopReason::Cancelled,
        Some("max_tokens") => StopReason::MaxTokens,
        Some("max_turn_requests") => StopReason::MaxTurnRequests,
        Some("refusal") => StopReason::Refusal,
        Some(_) | None => StopReason::Other,
    };
    let tokens =
        result.get("usage").and_then(|u| serde_json::from_value::<RawUsage>(u.clone()).ok()).map(|u| TokenUsage {
            input_tokens: u.input,
            output_tokens: u.output,
            cache_read_tokens: u.cached_read.unwrap_or(0),
            cache_write_tokens: u.cached_write.unwrap_or(0),
        });
    PromptResponse { stop_reason, tokens }
}

pub(crate) fn parse_session_update(params: &Value) -> SessionUpdate {
    let Some(update) = params.get("update").and_then(Value::as_object) else { return SessionUpdate::Other };
    let Some(kind) = update.get("sessionUpdate").and_then(Value::as_str) else { return SessionUpdate::Other };
    match kind {
        "user_message_chunk" => SessionUpdate::UserMessageChunk { text: chunk_text(update) },
        "agent_message_chunk" => SessionUpdate::AgentMessageChunk { text: chunk_text(update) },
        "agent_thought_chunk" => SessionUpdate::AgentThoughtChunk { text: chunk_text(update) },
        "tool_call" => parse_tool_wire(update).map_or(SessionUpdate::Other, SessionUpdate::ToolCall),
        "tool_call_update" => parse_tool_wire(update).map_or(SessionUpdate::Other, SessionUpdate::ToolCallUpdate),
        "current_mode_update" => update
            .get("currentModeId")
            .and_then(Value::as_str)
            .map_or(SessionUpdate::Other, |id| SessionUpdate::CurrentModeUpdate { mode_id: id.to_owned() }),
        "config_option_update" => {
            let config_options = update
                .get("configOptions")
                .and_then(|v| serde_json::from_value::<Vec<ConfigOption>>(v.clone()).ok())
                .unwrap_or_default();
            SessionUpdate::ConfigOptionUpdate { config_options }
        }
        "usage_update" => {
            match (update.get("used").and_then(Value::as_u64), update.get("size").and_then(Value::as_u64)) {
                (Some(used), Some(size)) => SessionUpdate::UsageUpdate { used, size },
                _ => SessionUpdate::Other,
            }
        }
        _ => SessionUpdate::Other,
    }
}

fn chunk_text(update: &serde_json::Map<String, Value>) -> String {
    update.get("content").and_then(|c| c.get("text")).and_then(Value::as_str).unwrap_or_default().to_owned()
}

pub(crate) fn parse_tool_wire(update: &serde_json::Map<String, Value>) -> Option<ToolCallWire> {
    let tool_call_id = update.get("toolCallId").and_then(Value::as_str)?.to_owned();
    let status = update.get("status").and_then(Value::as_str).map(|s| match s {
        "pending" => ToolStatusWire::Pending,
        "in_progress" => ToolStatusWire::InProgress,
        "completed" => ToolStatusWire::Completed,
        "failed" => ToolStatusWire::Failed,
        _ => ToolStatusWire::Other,
    });
    let kind = update.get("kind").and_then(Value::as_str).map(|k| match k {
        "read" => ToolKindWire::Read,
        "edit" => ToolKindWire::Edit,
        "delete" => ToolKindWire::Delete,
        "move" => ToolKindWire::Move,
        "search" => ToolKindWire::Search,
        "execute" => ToolKindWire::Execute,
        "think" => ToolKindWire::Think,
        "fetch" => ToolKindWire::Fetch,
        _ => ToolKindWire::Other,
    });
    let locations = update.get("locations").and_then(Value::as_array).map_or_default(|items| {
        items.iter().filter_map(|item| item.get("path").and_then(Value::as_str).map(str::to_owned)).collect()
    });
    let content_text = update.get("content").and_then(Value::as_array).and_then(|items| {
        let joined: Vec<&str> = items
            .iter()
            .filter_map(|item| item.get("content").or(Some(item)).and_then(|c| c.get("text")).and_then(Value::as_str))
            .collect();
        (!joined.is_empty()).then(|| joined.join("\n"))
    });
    Some(ToolCallWire {
        tool_call_id,
        status,
        kind,
        title: update.get("title").and_then(Value::as_str).map(str::to_owned),
        name: update.get("name").and_then(Value::as_str).map(str::to_owned),
        args: update.get("args").cloned(),
        raw_input: update.get("rawInput").cloned(),
        raw_output: update.get("rawOutput").cloned(),
        locations,
        content_text,
    })
}
