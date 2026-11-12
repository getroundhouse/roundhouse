//! Turns one line of Claude's stdout into a normalized event.
//!
//! This has to survive the CLI changing underneath it, so it reads loosely typed JSON and never
//! fails. Anything it doesn't recognize still reaches the log as `AgentOther` with the raw line.

use std::collections::HashMap;

use protocol::attachment::NewAttachment;
use protocol::event::{AgentRunState, ApprovalOutcome, ContentBlock, EventBody};
use protocol::model::{CacheRequest, MessageId, ToolUseId};
use protocol::tail::{BlockKind, DeltaChange, StreamDelta};
use serde_json::Value;

use calls::OpenCalls;
use compaction::Compactions;
pub(super) use limits::{whole_percent, window_named};
use pending::PendingCalls;
pub(crate) use prompt::PromptProgress;
use subagents::Subagents;
use surrogates::repair_lone_surrogates;
use tasks::Tasks;
use turn_end::{TurnModels, turn_ended};

mod advisor;
mod calls;
mod compaction;
mod file_change;
mod images;
mod limits;
mod pending;
mod prompt;
mod request;
mod result_text;
mod subagents;
mod surrogates;
mod tasks;
#[cfg(test)]
mod tests;
mod turn_end;
mod user;

/// What a line says, and what it shows without saying.
#[derive(Debug, PartialEq)]
pub struct Normalized {
    /// The calls this line showed the agent went on without. No line says so itself, so each goes in the log as a
    /// `ToolCallDropped` with no raw line, before what the line says.
    pub dropped: Vec<ToolUseId>,
    pub parsed: Parsed,
    /// The list of background tasks again, once a subagent's start, which is what the line says, names the call that
    /// started a listed task. It goes in the log after what the line says, with no raw line, since the line is that.
    pub relisted: Option<EventBody>,
}

#[derive(Debug, PartialEq)]
pub enum Parsed {
    /// Goes in the log, with the raw line.
    Event(EventBody),
    /// Goes in the log like an `Event`, which names images a tool sent back. Their bytes go to the hub beside it, and
    /// `raw` is the line without them.
    EventWithImages { body: EventBody, images: Vec<NewAttachment>, raw: String },
    /// Live only. The finished message replaces these.
    Delta(StreamDelta),
    /// The CLI is waiting on an answer the adapter doesn't support. It needs an error reply, or
    /// the session hangs.
    UnsupportedControl { request_id: String, subtype: String },
    /// Carries nothing worth storing.
    Drop,
    /// The CLI's answer to a control request of ours: what it sent back, or why it refused.
    ControlReply { request_id: String, answer: Result<Value, String> },
    /// Where a prompt the adapter sent has got to, by the `uuid` the adapter gave it. Only the adapter knows which
    /// recorded prompt that is.
    Prompt { uuid: String, progress: PromptProgress },
    /// One of the agent's own requests to its model, as the model started to answer it.
    Request(CacheRequest),
}

/// Parses lines in the order they came, and remembers which block of each message is streaming. Claude
/// sends a block's deltas, then the finished block, so the finished block can say where it sits in its
/// message, as its deltas did. It keeps which calls are open, from each call to its result. It says what a
/// background task is, and that it stopped one, apart from the list of them, so the list can say which call started
/// each, which are monitors, by that call, and which left without finishing. It remembers which task each subagent runs as, so
/// the end of one can say which subagent it was. It never says a call was dropped, so what follows the call has to
/// show it. And its `result` doesn't say which models the agent's own replies came from, so the replies have to. It
/// holds what a compaction's boundary says until the summary after it, so the two are one event.
#[derive(Debug, Default)]
pub struct Normalizer {
    /// The block of each message that's streaming, and the call whose subagent is writing it, if one is.
    streaming: HashMap<MessageId, (u32, Option<ToolUseId>)>,
    calls: OpenCalls,
    tasks: Tasks,
    pending_calls: PendingCalls,
    models: TurnModels,
    subagents: Subagents,
    compactions: Compactions,
}

impl Normalizer {
    pub fn line(&mut self, line: &str) -> Normalized {
        let Some(value) = value_of(line) else {
            return Normalized { dropped: Vec::new(), parsed: unparsed(), relisted: None };
        };
        self.calls.note(&value);
        let dropped = self.pending_calls.note(&value);
        let noted = self.tasks.note(&value, &self.calls);
        if let Some(body) = self.subagents.read(&value) {
            let relisted = noted.map(|mut list| {
                self.list(&mut list);
                list
            });
            return Normalized { dropped, parsed: Parsed::Event(body), relisted };
        }
        if let Some(parsed) = self.compactions.read(&value) {
            return Normalized { dropped, parsed, relisted: None };
        }
        let mut parsed = noted.map_or_else(|| parse_value(&value), Parsed::Event);
        if let Parsed::Event(body) = &mut parsed {
            self.list(body);
        }
        if let Parsed::Delta(delta) = &parsed {
            match delta.change {
                DeltaChange::BlockStart { index, .. }
                | DeltaChange::Text { index, .. }
                | DeltaChange::Thinking { index, .. }
                | DeltaChange::ToolInput { index, .. } => {
                    self.streaming.insert(delta.message_id.clone(), (index, delta.parent_tool_use_id.clone()));
                }
                DeltaChange::MessageStop => {
                    self.streaming.remove(&delta.message_id);
                }
                DeltaChange::Unknown => {}
            }
        }
        if let Parsed::Event(EventBody::Assistant { message_id: Some(id), blocks, first_index, .. }) = &mut parsed {
            // The block that streamed is this one, so no later block can take its place. Claude sends one block
            // per event, and if several come at once, which of them streamed can't be told.
            let streamed = self.streaming.remove(id).map(|(index, _)| index);
            *first_index = streamed.filter(|_| blocks.len() == 1);
        }
        if let Parsed::Event(body) = &parsed {
            self.models.note(body);
        }
        // Whatever a turn left streaming, such as when it was cancelled, is over, but for what a subagent in the
        // background is writing, since it carries on.
        if let Parsed::Event(EventBody::TurnEnded { models, .. }) = &mut parsed {
            *models = self.models.take();
            let subagents = &self.subagents;
            self.streaming.retain(|_, (_, writer)| writer.as_ref().is_some_and(|call| subagents.in_background(call)));
            self.pending_calls.turn_ended();
            self.calls.turn_ended();
        }
        Normalized { dropped, parsed, relisted: None }
    }

    /// Gives a list of background tasks what the lines before it said of them.
    fn list(&mut self, body: &mut EventBody) {
        if let EventBody::AgentTasks { tasks, stopped } = body {
            *stopped = self.tasks.listed(tasks);
        }
    }
}

/// One line on its own, for tests of what needs nothing before it. A [`Normalizer`] also says where a finished
/// block sits in its message, and which background tasks are monitors or were stopped.
#[cfg(test)]
fn parse_line(line: &str) -> Parsed {
    value_of(line).map_or_else(unparsed, |value| parse_value(&value))
}

/// A line as JSON, with what the CLI can write that JSON can't hold repaired.
pub(crate) fn value_of(line: &str) -> Option<Value> {
    serde_json::from_str::<Value>(line)
        // JavaScript strings can hold half of a surrogate pair, and JSON.stringify writes it as an
        // escape. That's rare, and it can land in a line that matters, such as an approval.
        .or_else(|_| serde_json::from_str::<Value>(&repair_lone_surrogates(line)))
        .ok()
}

fn unparsed() -> Parsed {
    Parsed::Event(EventBody::AgentOther { kind: "unparsed".into(), parent_tool_use_id: None })
}

fn parse_value(value: &Value) -> Parsed {
    let kind = str_of(value, "type").unwrap_or("unknown");
    match kind {
        "assistant" => Parsed::Event(assistant(value)),
        "user" => user::line(value),
        "result" => Parsed::Event(turn_ended(value)),
        "system" => system(value),
        "stream_event" => stream_event(value),
        "control_request" => control_request(value),
        "control_cancel_request" => match str_of(value, "request_id") {
            Some(id) => Parsed::Event(EventBody::ApprovalResolved {
                approval_id: id.into(),
                outcome: ApprovalOutcome::Cancelled,
                answers_by_text: None,
                answers_by_position: None,
            }),
            None => other(kind),
        },
        // Replies to the adapter's own requests, such as an interrupt.
        "control_response" => control_reply(value),
        "command_lifecycle" => prompt::command_lifecycle(value),
        "rate_limit_event" => limits::rate_limit_event(value),
        _ => other(kind),
    }
}

/// A line of `kind` with no normalized form, which isn't a subagent's. Not every line's `parent_tool_use_id` names a
/// subagent: a `tool_progress` line's names the call that's making progress.
fn other(kind: &str) -> Parsed {
    Parsed::Event(EventBody::AgentOther { kind: kind.to_string(), parent_tool_use_id: None })
}

fn str_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn id_of<T: From<String>>(value: &Value, key: &str) -> Option<T> {
    string_of(value, key).map(T::from)
}

fn string_of(value: &Value, key: &str) -> Option<String> {
    str_of(value, key).map(str::to_string)
}

/// Changes every string in `value`, wherever it is.
fn each_string(value: &mut Value, change: &mut impl FnMut(&mut String)) {
    match value {
        Value::String(text) => change(text),
        Value::Array(items) => items.iter_mut().for_each(|item| each_string(item, change)),
        Value::Object(fields) => fields.values_mut().for_each(|field| each_string(field, change)),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn control_reply(value: &Value) -> Parsed {
    let response = value.get("response").unwrap_or(&Value::Null);
    match str_of(response, "request_id") {
        Some(request_id) => Parsed::ControlReply {
            request_id: request_id.to_string(),
            answer: if str_of(response, "subtype") == Some("error") {
                Err(str_of(response, "error").unwrap_or("the agent refused").to_string())
            } else {
                Ok(response.get("response").cloned().unwrap_or(Value::Null))
            },
        },
        None => Parsed::Drop,
    }
}

fn system(value: &Value) -> Parsed {
    let subtype = str_of(value, "subtype").unwrap_or("unknown");
    match subtype {
        "init" => Parsed::Event(EventBody::AgentInit {
            agent_session_id: id_of(value, "session_id"),
            model: string_of(value, "model"),
            cwd: string_of(value, "cwd"),
            permission_mode: str_of(value, "permissionMode").map(super::mode::mode),
            agent_version: string_of(value, "claude_code_version"),
        }),
        "session_state_changed" => Parsed::Event(EventBody::AgentState {
            state: match str_of(value, "state") {
                Some("idle") => AgentRunState::Idle,
                Some("running") => AgentRunState::Running,
                Some("requires_action") => AgentRunState::RequiresAction,
                _ => AgentRunState::Unknown,
            },
        }),
        "background_tasks_changed" => tasks::listed(value),
        "status" => match str_of(value, "permissionMode") {
            Some(mode) => Parsed::Event(EventBody::AgentMode { permission_mode: super::mode::mode(mode) }),
            None => other("system/status"),
        },
        // A running token count for a spinner. Several arrive per second while thinking.
        "thinking_tokens" => Parsed::Drop,
        other_subtype => other(&format!("system/{other_subtype}")),
    }
}

/// The CLI's name for the model of a message it wrote itself, such as one saying the API failed. No model wrote it, so
/// it's recorded as naming none, and a turn or a subagent isn't taken to have run on it.
const NO_MODEL: &str = "<synthetic>";

fn assistant(value: &Value) -> EventBody {
    let message = value.get("message").unwrap_or(&Value::Null);
    EventBody::Assistant {
        message_id: id_of(message, "id"),
        parent_tool_use_id: id_of(value, "parent_tool_use_id"),
        model: str_of(message, "model").filter(|model| *model != NO_MODEL).map(str::to_string),
        blocks: blocks(message.get("content")),
        // A Normalizer knows it from the deltas before.
        first_index: None,
    }
}

fn blocks(content: Option<&Value>) -> Vec<ContentBlock> {
    match content {
        Some(Value::String(text)) => vec![ContentBlock::Text { text: text.clone(), images: Vec::new() }],
        Some(Value::Array(items)) => items.iter().map(block).collect(),
        _ => Vec::new(),
    }
}

fn block(value: &Value) -> ContentBlock {
    match str_of(value, "type") {
        Some("text") => ContentBlock::Text { text: string_of(value, "text").unwrap_or_default(), images: Vec::new() },
        Some("thinking") => ContentBlock::Thinking { text: string_of(value, "thinking").unwrap_or_default() },
        // A tool the API runs itself, such as the advisor, is called the same way.
        Some("tool_use" | "server_tool_use") => ContentBlock::ToolUse {
            id: string_of(value, "id").unwrap_or_default().into(),
            name: string_of(value, "name").unwrap_or_default(),
            input: value.get("input").cloned().unwrap_or(Value::Null),
        },
        Some("tool_result") => ContentBlock::ToolResult {
            tool_use_id: string_of(value, "tool_use_id").unwrap_or_default().into(),
            is_error: value.get("is_error").and_then(Value::as_bool).unwrap_or(false),
            text: result_text::read(value.get("content")),
            // Only a user line's results have images, and `images` takes them from it.
            images: Vec::new(),
            change: None,
            encrypted: false,
        },
        Some("advisor_tool_result") => advisor::result(value),
        Some("image") => {
            ContentBlock::Image { media_type: value.get("source").and_then(|s| string_of(s, "media_type")) }
        }
        _ => ContentBlock::Unknown,
    }
}

fn control_request(value: &Value) -> Parsed {
    let request = value.get("request").unwrap_or(&Value::Null);
    let request_id = string_of(value, "request_id").unwrap_or_default();
    let subtype = str_of(request, "subtype").unwrap_or("unknown");
    if subtype != "can_use_tool" {
        return Parsed::UnsupportedControl { request_id, subtype: subtype.to_string() };
    }
    let tool_name = string_of(request, "tool_name").unwrap_or_default();
    let input = request.get("input").cloned().unwrap_or(Value::Null);
    Parsed::Event(EventBody::ApprovalRequested {
        approval_id: request_id.into(),
        asking: Some(Box::new(super::approvals::asking(&tool_name, &input))),
        tool_name,
        tool_use_id: id_of(request, "tool_use_id"),
        input,
        description: string_of(request, "description"),
        suggestions: request.get("permission_suggestions").cloned().unwrap_or(Value::Null),
    })
}

fn stream_event(value: &Value) -> Parsed {
    let event = value.get("event").unwrap_or(&Value::Null);
    if let Some(request) = request::request(value, event) {
        return Parsed::Request(request);
    }
    let message_id =
        string_of(value, "api_message_id").or_else(|| event.get("message").and_then(|m| string_of(m, "id")));
    let Some(message_id) = message_id else {
        return Parsed::Drop;
    };
    let index = event.get("index").and_then(Value::as_u64).and_then(|i| u32::try_from(i).ok());
    let change = match (str_of(event, "type"), index) {
        (Some("content_block_start"), Some(index)) => {
            let block = event.get("content_block").unwrap_or(&Value::Null);
            let kind = match str_of(block, "type") {
                Some("text") => BlockKind::Text,
                Some("thinking") => BlockKind::Thinking,
                Some("tool_use" | "server_tool_use") => BlockKind::ToolUse,
                _ => BlockKind::Unknown,
            };
            DeltaChange::BlockStart { index, kind, tool_name: string_of(block, "name") }
        }
        (Some("content_block_delta"), Some(index)) => {
            let delta = event.get("delta").unwrap_or(&Value::Null);
            match str_of(delta, "type") {
                Some("text_delta") => DeltaChange::Text { index, text: string_of(delta, "text").unwrap_or_default() },
                Some("thinking_delta") => {
                    DeltaChange::Thinking { index, text: string_of(delta, "thinking").unwrap_or_default() }
                }
                Some("input_json_delta") => {
                    DeltaChange::ToolInput { index, json: string_of(delta, "partial_json").unwrap_or_default() }
                }
                _ => return Parsed::Drop,
            }
        }
        (Some("message_stop"), _) => DeltaChange::MessageStop,
        // A subagent's message_start, content_block_stop and message_delta add nothing to the tail.
        _ => return Parsed::Drop,
    };
    Parsed::Delta(StreamDelta {
        // The hub knows the run, and sets it.
        run: None,
        message_id: message_id.into(),
        parent_tool_use_id: id_of(value, "parent_tool_use_id"),
        change,
    })
}
