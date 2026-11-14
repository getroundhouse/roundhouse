use super::{agent, command, files, older, other, search, web};
use agent::Agent;
use command::Command;
use files::{EditFile, ReadFile, WriteFile};
use other::Other;
use protocol::event::{Asking, CallKind, ContentBlock, EventBody};
use search::{Looks, Search};
use serde_json::Value;
use std::borrow::Cow;
use web::{Fetch, WebSearch};

/// A call, by what kind of thing it does.
pub enum ToolCall<'a> {
    Command(Command<'a>),
    Read(ReadFile<'a>),
    Write(WriteFile<'a>),
    Edit(EditFile<'a>),
    Search(Search<'a>),
    Agent(Agent<'a>),
    Fetch(Fetch<'a>),
    WebSearch(WebSearch<'a>),
    /// Asking the advisor, a stronger model that reads the whole conversation and advises the agent. It takes no
    /// input.
    Advisor,
    Other(Other),
    Typed(&'a protocol::capability::calls::Call),
}

/// A call as it's recorded: what its adapter said it is, if it said, the agent's own name for the tool, and what the
/// agent gave it. It's made only from the block that makes the call, or the request to make it, so the kind an adapter
/// said is never left behind, and a call is read from its tool's name only when it was recorded before adapters said.
#[derive(Clone, Copy, Debug)]
pub enum Recorded<'a> {
    Legacy { kind: Option<&'a CallKind>, name: &'a str, input: &'a Value },
    Typed(&'a protocol::capability::calls::Call),
}

impl<'a> Recorded<'a> {
    /// The call `block` makes, or `None` for any other block.
    pub fn of(block: &'a ContentBlock) -> Option<Self> {
        match block {
            ContentBlock::ToolUse { name, input, call, .. } => Some(Self::Legacy { kind: call.as_ref(), name, input }),
            ContentBlock::Call { call, .. } => Some(Self::Typed(call)),
            ContentBlock::Text { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::CallResult { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::Image { .. }
            | ContentBlock::Unknown => None,
        }
    }

    /// The call the request for approval `body` asks to make, or `None` for any other event. A request that asks
    /// something else is one too, to the tool that asks it, and what it asks says its kind. Only one that says no
    /// kind, which is one recorded before adapters said, leaves the kind to its tool's name.
    pub fn asked(body: &'a EventBody) -> Option<Self> {
        let EventBody::ApprovalRequested { tool_name, input, asking, request, .. } = body else { return None };
        if let Some(request) = request {
            return match &request.asks {
                protocol::capability::approvals::Asks::Call { call } => Some(Self::Typed(call)),
                protocol::capability::approvals::Asks::Questions { .. }
                | protocol::capability::approvals::Asks::Plan { .. }
                | protocol::capability::approvals::Asks::Network { .. }
                | protocol::capability::approvals::Asks::Access { .. }
                | protocol::capability::approvals::Asks::Server(_)
                | protocol::capability::approvals::Asks::Unknown => None,
            };
        }
        Some(Self::Legacy { kind: asking.as_deref().and_then(Asking::call), name: tool_name, input })
    }

    /// What kind of call it is: what its adapter said, or for one recorded before adapters said, what Claude's tool of
    /// that name was.
    /// Whether it hands work to a subagent.
    pub fn hands_off(&self) -> bool {
        match self {
            Self::Typed(call) => matches!(call, protocol::capability::calls::Call::Subagent(_)),
            Self::Legacy { kind, name, .. } => {
                kind.map_or_else(|| older::kind_of(name), Clone::clone) == CallKind::Subagent
            }
        }
    }

    /// Whether it asks the person something, questions or to approve a plan, which its request for approval carries
    /// in full, so a client draws that rather than the call.
    pub fn asks_the_person(&self) -> bool {
        match self {
            Self::Typed(call) => {
                matches!(call, protocol::capability::calls::Call::Questions | protocol::capability::calls::Call::Plan)
            }
            Self::Legacy { kind, name, .. } => {
                matches!(kind.map_or_else(|| older::kind_of(name), Clone::clone), CallKind::Questions | CallKind::Plan)
            }
        }
    }

    pub fn input(&self) -> &'a Value {
        match self {
            Self::Legacy { input, .. } => input,
            Self::Typed(
                protocol::capability::calls::Call::Mcp { input, .. }
                | protocol::capability::calls::Call::Other { input, .. },
            ) => input,
            Self::Typed(_) => &Value::Null,
        }
    }

    /// One line saying what it's about, as [`ToolCall::summary`] says.
    pub fn summary(&self) -> Option<String> {
        ToolCall::read(*self).summary()
    }
}

/// What a request to use a tool is about, on one line: what the agent said it's for, or what its call says, the same
/// in every client. `None` for any other event, and for a call with nothing to say.
pub fn asked_about(body: &EventBody) -> Option<String> {
    let EventBody::ApprovalRequested { description, .. } = body else { return None };
    description.clone().or_else(|| Recorded::asked(body)?.summary())
}

impl<'a> ToolCall<'a> {
    pub fn read(call: Recorded<'a>) -> Self {
        let Recorded::Legacy { kind, name, input } = call else {
            let Recorded::Typed(call) = call else { unreachable!() };
            return super::typed::read(call);
        };
        let kind = kind.map_or_else(|| Cow::Owned(older::kind_of(name)), Cow::Borrowed);
        let known = match &*kind {
            CallKind::Command => Command::read(input).map(ToolCall::Command),
            CallKind::Read => ReadFile::read(input).map(ToolCall::Read),
            CallKind::Write => WriteFile::read(input).map(ToolCall::Write),
            CallKind::Edit => EditFile::read(input).map(ToolCall::Edit),
            CallKind::Search => Search::read(input, Looks::InFiles).map(ToolCall::Search),
            CallKind::FindFiles => Search::read(input, Looks::ForNames).map(ToolCall::Search),
            CallKind::Fetch => Fetch::read(input).map(ToolCall::Fetch),
            CallKind::WebSearch => WebSearch::read(input).map(ToolCall::WebSearch),
            CallKind::Advisor => Some(ToolCall::Advisor),
            // Always drawn as one, so the subagent's work, which its rows leave to the call, shows somewhere.
            CallKind::Subagent => Some(ToolCall::Agent(Agent::read(input))),
            // What these ask is drawn from their requests for approval. A client that draws the call too draws it as
            // any tool.
            CallKind::Questions | CallKind::Plan | CallKind::Other { .. } | CallKind::Unknown => None,
        };
        // A known kind whose input isn't what it should be is still drawn, as any tool would be.
        known.unwrap_or_else(|| ToolCall::Other(Other::read(&kind, name, input)))
    }

    /// One line saying what the call is about, the same wherever it's shown, such as where a client asks whether to
    /// run it or in a terminal: the input that matters most for its kind, or for a subagent what it's for. A tool
    /// without a kind of its own has what [`Other`] says. `None` for a call with nothing to say, such as the advisor,
    /// which takes no input.
    pub fn summary(&self) -> Option<String> {
        let about = match self {
            ToolCall::Command(command) => command.summary(),
            ToolCall::Read(ReadFile { path })
            | ToolCall::Write(WriteFile { path, .. })
            | ToolCall::Edit(EditFile { path, .. }) => path,
            ToolCall::Search(search) => search.pattern,
            ToolCall::Agent(agent) => agent.described().or(agent.prompt)?,
            ToolCall::Fetch(fetch) => fetch.url,
            ToolCall::WebSearch(search) => search.query,
            ToolCall::Advisor => return None,
            ToolCall::Other(other) => return other.summary.clone(),
            ToolCall::Typed(call) => return Some(super::typed::summary(call)),
        };
        Some(first_line(about).to_string())
    }
}

/// Inputs that usually say what a call is about, in the order they're looked for.
const MAIN_INPUTS: &[&str] = &["command", "file_path", "path", "pattern", "url", "query", "prompt"];

/// The usual input that says most about what a call is about, on one line, if it has one.
pub(super) fn main_input(input: &Value) -> Option<&str> {
    MAIN_INPUTS.iter().find_map(|key| str_field(input, key)).map(first_line)
}

/// What the agent said a call is for, in its own words, if it said.
pub(super) fn description(input: &Value) -> Option<&str> {
    str_field(input, "description").map(str::trim).filter(|d| !d.is_empty())
}

/// How many of something there are, in words, such as `1 line` or `3 lines`.
pub fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

/// The first line of `text` with anything on it, trimmed, which is all a line about a call has room for.
pub fn first_line(text: &str) -> &str {
    text.lines().find(|line| !line.trim().is_empty()).unwrap_or("").trim()
}

pub(super) fn str_field<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}
