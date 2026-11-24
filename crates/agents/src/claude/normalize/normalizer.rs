use super::super::typed;
use super::{
    blocks, calls, compaction, denied, limits, own_turns, pending, prompt, request, retrying, said, subagents,
    surrogates, tasks, turn_end, user,
};
use calls::OpenCalls;
use compaction::Compactions;
use own_turns::OwnTurns;
use pending::PendingCalls;
use prompt::PromptProgress;
use protocol::attachment::NewAttachment;
use protocol::capability::approvals::ApprovalMode;
use protocol::capability::approvals::Asks;
use protocol::event::{AgentRunState, EventBody, TurnOutcome};
use protocol::model::{CacheRequest, MessageId, SubagentId, ToolUseId, UsageLimit};
use protocol::tail::{BlockKind, DeltaChange, Retrying, StreamDelta};
use said::FailedRequests;
use serde_json::Value;
use std::collections::HashMap;
use subagents::Subagents;
use surrogates::repair_lone_surrogates;
use tasks::Tasks;
use turn_end::{TurnModels, turn_ended};

/// What a line says, and what it shows without saying.
#[derive(Debug, PartialEq)]
pub struct Normalized {
    /// The calls this line showed the agent went on without. No line says so itself, so each goes in the log as a
    /// `ToolCallDropped` with no raw line, before what the line says.
    pub dropped: Vec<ToolUseId>,
    pub parsed: Parsed,
    /// What the line shows without saying, which goes in the log after what it says, with no raw line, since the line
    /// is that: the list of background tasks again, once a subagent's start, which is what the line says, names the call
    /// that started a listed task, or why the agent starts a turn by itself, after the turn's `init`.
    pub after: Option<EventBody>,
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
    /// Live only: the CLI is trying a failed request again, until the agent writes again.
    Retrying(Retrying),
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
    /// How much of the account's limits is used, as the CLI said by itself, each window it named.
    Limits(Vec<UsageLimit>),
}

/// Parses lines in the order they came, and remembers which block of each message is streaming. Claude
/// sends a block's deltas, then the finished block, so the finished block can say where it sits in its
/// message, as its deltas did. It keeps which calls are open, from each call to its result. It says what a
/// background task is, and that it stopped one, apart from the list of them, so the list can say which call started
/// each, which are monitors, by that call, and which left without finishing. It remembers which task each subagent runs as, so
/// the end of one can say which subagent it was. It never says a call was dropped, so what follows the call has to
/// show it. And its `result` doesn't say which models the agent's own replies came from, so the replies have to. It
/// holds what a compaction's boundary says until the summary after it, so the two are one event. And it holds the end
/// of each of the agent's own background tasks until a request to the model takes it in, so a turn the agent starts by
/// itself can say which it took in.
#[derive(Debug, Default)]
pub struct Normalizer {
    /// The block of each message that's streaming, and the subagent writing it, if one is.
    pub(super) streaming: HashMap<MessageId, (u32, Option<SubagentId>)>,
    pub(super) calls: OpenCalls,
    pub(super) tasks: Tasks,
    pub(super) pending_calls: PendingCalls,
    pub(super) models: TurnModels,
    pub(super) subagents: Subagents,
    pub(super) compactions: Compactions,
    pub(super) failed: FailedRequests,
    pub(super) own_turns: OwnTurns,
    /// The mode the CLI last said it's in, in which it asks to make a call only when it asks whatever the mode.
    pub(super) mode: Option<ApprovalMode>,
}

impl Normalizer {
    pub fn line(&mut self, line: &str) -> Normalized {
        let Some(value) = value_of(line) else {
            return Normalized { dropped: Vec::new(), parsed: unparsed(), after: None };
        };
        let answered = self.calls.note(&value);
        let dropped = self.pending_calls.note(&value);
        let own_turn = self.own_turns.note(&value, &self.tasks);
        let noted = self.tasks.note(&value, &self.calls);
        let in_foreground = self.tasks.in_foreground(&value);
        if let Some(body) = self.subagents.read(&value) {
            let relisted = noted.map(|mut list| {
                self.list(&mut list);
                list
            });
            // A list said again comes only after a subagent's line, and a turn's reason only after its `init`.
            return Normalized { dropped, parsed: Parsed::Event(body), after: relisted.or(own_turn) };
        }
        if let Some(parsed) = self.compactions.read(&value) {
            return Normalized { dropped, parsed, after: own_turn };
        }
        let mut parsed = match noted {
            Some(list) => Parsed::Event(list),
            None => tasks::changed(&value, in_foreground).unwrap_or_else(|| parse_value(&value, &self.subagents)),
        };
        if let Parsed::Event(body) = &mut parsed {
            self.list(body);
        }
        if let Parsed::Event(body) | Parsed::EventWithImages { body, .. } = &mut parsed {
            denied::say(body, &value, &answered);
            typed::say(body, &answered, &value);
        }
        self.asking(&mut parsed, &value);
        if let Parsed::Delta(delta) = &parsed {
            self.streamed(delta);
        }
        if let Parsed::Event(EventBody::Assistant { message_id: Some(id), blocks, first_index, .. }) = &mut parsed {
            // The block that streamed is this one, so no later block can take its place. Claude sends one block
            // per event, and if several come at once, which of them streamed can't be told.
            let streamed = self.streaming.remove(id).map(|(index, _)| index);
            *first_index = streamed.filter(|_| blocks.len() == 1);
        }
        if let Parsed::Event(body) = &parsed {
            self.models.note(body);
            self.failed.note(body);
        }
        // Whatever a turn left streaming, such as when it was cancelled, is over, but for what a subagent in the
        // background is writing, since it carries on.
        if let Parsed::Event(EventBody::TurnEnded { models, outcome, request_failed, .. }) = &mut parsed {
            *models = self.models.take();
            *request_failed = self.failed.ended_on_one() && *outcome == TurnOutcome::Error;
            let subagents = &self.subagents;
            self.streaming
                .retain(|_, (_, writer)| writer.as_ref().is_some_and(|writer| subagents.in_background(writer)));
            self.pending_calls.turn_ended();
            self.calls.turn_ended();
        }
        Normalized { dropped, parsed, after: own_turn }
    }

    /// Notes which block of its message `delta` streams, or that the message is over.
    fn streamed(&mut self, delta: &StreamDelta) {
        match delta.change {
            DeltaChange::BlockStart { index, .. }
            | DeltaChange::Text { index, .. }
            | DeltaChange::Thinking { index, .. }
            | DeltaChange::ToolInput { index, .. } => {
                self.streaming.insert(delta.message_id.clone(), (index, delta.subagent.clone()));
            }
            DeltaChange::MessageStop => {
                self.streaming.remove(&delta.message_id);
            }
            DeltaChange::Unknown => {}
        }
    }

    /// Notes the mode the CLI says it's in, and says of a request in `parsed`, the line `value`, which subagent asks
    /// it, and whether it's asked whatever the mode.
    fn asking(&mut self, parsed: &mut Parsed, value: &Value) {
        if let Parsed::Event(EventBody::AgentInit { mode: Some(mode), .. } | EventBody::AgentMode { mode }) = parsed {
            self.mode = Some(*mode);
        }
        if let Parsed::Event(EventBody::ApprovalRequested { asked_by, request, .. }) = parsed {
            *asked_by = Subagents::asked_by(value);
            // With full access, the CLI asks to make a call only where it asks whatever the mode, as its check of a
            // removal does, which a CLI from before it said so doesn't mark.
            if self.mode == Some(super::super::claims::full_access()) && matches!(request.asks, Asks::Call { .. }) {
                request.always = true;
                request.suggested.clear();
            }
        }
    }

    /// Gives a list of background tasks what the lines before it said of them.
    pub(super) fn list(&mut self, body: &mut EventBody) {
        if let EventBody::AgentTasks { tasks, stopped } = body {
            *stopped = self.tasks.listed(tasks);
        }
    }
}

/// One line on its own, for tests of what needs nothing before it. A [`Normalizer`] also says where a finished
/// block sits in its message, and which background tasks are monitors or were stopped.
#[cfg(test)]
pub(super) fn parse_line(line: &str) -> Parsed {
    value_of(line).map_or_else(unparsed, |value| parse_value(&value, &Subagents::default()))
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
    Parsed::Event(EventBody::AgentOther { kind: "unparsed".into(), subagent: None })
}

/// What `value` says, naming the subagent whose line it is as `subagents` know it.
fn parse_value(value: &Value, subagents: &Subagents) -> Parsed {
    let kind = str_of(value, "type").unwrap_or("unknown");
    let from = subagents.of_line(value);
    match kind {
        "assistant" => Parsed::Event(said::message(value, from.clone()).unwrap_or_else(|| assistant(value, from))),
        "user" => user::line(value, from),
        "result" => Parsed::Event(turn_ended(value)),
        "system" => system(value),
        "stream_event" => stream_event(value, from),
        "control_request" => control_request(value),
        "control_cancel_request" => match str_of(value, "request_id") {
            // Nobody decided it.
            Some(id) => {
                Parsed::Event(EventBody::ApprovalResolved { approval_id: id.into(), decision: None, by_setting: false })
            }
            None => other(kind),
        },
        // Replies to the adapter's own requests, such as an interrupt.
        "control_response" => control_reply(value),
        "command_lifecycle" => prompt::command_lifecycle(value),
        "rate_limit_event" => limits::rate_limit_event(value),
        // How long a call has been running, every few seconds while it runs. The call's start and result say
        // everything the log keeps, and a client times a running call itself.
        "tool_progress" => Parsed::Drop,
        _ => other(kind),
    }
}

/// A line of `kind` with no normalized form, which isn't a subagent's, since not every line's `parent_tool_use_id`
/// names a subagent.
pub(super) fn other(kind: &str) -> Parsed {
    Parsed::Event(EventBody::AgentOther { kind: kind.to_string(), subagent: None })
}

pub(super) fn str_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

pub(super) fn id_of<T: From<String>>(value: &Value, key: &str) -> Option<T> {
    string_of(value, key).map(T::from)
}

pub(super) fn string_of(value: &Value, key: &str) -> Option<String> {
    str_of(value, key).map(str::to_string)
}

/// Changes every string in `value`, wherever it is.
pub(super) fn each_string(value: &mut Value, change: &mut impl FnMut(&mut String)) {
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
            // The worker says what the run claims as it starts it.
            agent_session_id: id_of(value, "session_id"),
            model: string_of(value, "model"),
            cwd: string_of(value, "cwd"),
            mode: str_of(value, "permissionMode").map(super::super::mode::mode),
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
        "status" => match (str_of(value, "permissionMode"), str_of(value, "status")) {
            (Some(mode), _) => Parsed::Event(EventBody::AgentMode { mode: super::super::mode::mode(mode) }),
            // Sent as each request to the model starts, which the run's state already says is under way. Noting
            // which background tasks the request takes in reads the line before this.
            (None, Some("requesting")) => Parsed::Drop,
            (None, _) => other("system/status"),
        },
        "api_retry" => retrying::read(value),
        // A running token count for a spinner. Several arrive per second while thinking.
        "thinking_tokens"
        // What the CLI's own interface shows of git and GitHub: that the agent committed, pushed, rebased or merged,
        // and that it opened, pushed to, commented on or merged a pull request. The worker reads git itself, which
        // the hub learns of a pull request from, and the call that did it is recorded.
        | "vcs_state_changed"
        | "code_change_published" => Parsed::Drop,
        other_subtype => other(&format!("system/{other_subtype}")),
    }
}

fn assistant(value: &Value, subagent: Option<SubagentId>) -> EventBody {
    let message = value.get("message").unwrap_or(&Value::Null);
    EventBody::Assistant {
        message_id: id_of(message, "id"),
        subagent,
        model: string_of(message, "model"),
        blocks: blocks::read(message.get("content")),
        // A Normalizer knows it from the deltas before.
        first_index: None,
    }
}

fn control_request(value: &Value) -> Parsed {
    let request = value.get("request").unwrap_or(&Value::Null);
    let request_id = string_of(value, "request_id").unwrap_or_default();
    let subtype = str_of(request, "subtype").unwrap_or("unknown");
    if subtype != "can_use_tool" {
        return Parsed::UnsupportedControl { request_id, subtype: subtype.to_string() };
    }
    let tool = string_of(request, "tool_name").unwrap_or_default();
    let input = request.get("input").cloned().unwrap_or(Value::Null);
    Parsed::Event(EventBody::ApprovalRequested {
        approval_id: request_id.into(),
        request: Box::new(super::super::approvals::request(&tool, &input, request)),
        tool_use_id: id_of(request, "tool_use_id"),
        // A Normalizer knows it from the subagents it's heard start.
        asked_by: None,
    })
}

fn stream_event(value: &Value, subagent: Option<SubagentId>) -> Parsed {
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
        subagent,
        change,
    })
}
