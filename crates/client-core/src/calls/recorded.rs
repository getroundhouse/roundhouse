use super::{agent, command, files, other, search, subagents, web};
use agent::Agent;
use command::Command;
use files::{EditFile, ReadFile, WriteFile};
use other::Other;
use protocol::capability::approvals::Asks;
use protocol::capability::calls::Call;
use protocol::capability::calls::call::{GenerateImage, NotebookEdit, Patch};
use protocol::event::{ContentBlock, EventBody};
use search::Search;
use serde_json::Value;
use subagents::ToSubagents;
use web::{Fetch, WebSearch};

/// A call, by what kind of thing it does.
pub enum ToolCall<'a> {
    Command(Command<'a>),
    Read(ReadFile<'a>),
    Write(WriteFile<'a>),
    Edit(EditFile<'a>),
    Search(Search<'a>),
    Agent(Agent<'a>),
    /// Telling a subagent at work something, waiting for some, or stopping one.
    ToSubagents(ToSubagents<'a>),
    Fetch(Fetch<'a>),
    WebSearch(WebSearch<'a>),
    /// Asking the advisor, a stronger model that reads the whole conversation and advises the agent. It takes no
    /// input.
    Advisor,
    /// Changing files with a patch, as Codex does: several at once, each added, deleted, or changed and maybe moved.
    Patch(&'a Patch),
    /// Changing one cell of a notebook.
    NotebookEdit(&'a NotebookEdit),
    /// Making an image, from what it was asked for. Codex says that only once it's made, in what came back.
    MakeImage(&'a GenerateImage),
    Other(Other),
    Typed(&'a protocol::capability::calls::Call),
}

/// A call as it's recorded, typed, as its agent's adapter records it. It's made only from the block that makes the
/// call, or the request to make it.
#[derive(Clone, Copy, Debug)]
pub enum Recorded<'a> {
    Typed(&'a Call),
}

impl<'a> Recorded<'a> {
    /// The call `block` makes, or `None` for any other block.
    pub fn of(block: &'a ContentBlock) -> Option<Self> {
        let ContentBlock::Call { call, .. } = block else { return None };
        Some(Self::Typed(call))
    }

    /// The call the request for approval `body` asks to make, or `None` for one that asks something else and for any
    /// other event.
    pub fn asked(body: &'a EventBody) -> Option<Self> {
        let EventBody::ApprovalRequested { request, .. } = body else { return None };
        match &request.asks {
            Asks::Call { call } => Some(Self::Typed(call)),
            Asks::Questions { .. }
            | Asks::Plan { .. }
            | Asks::Network { .. }
            | Asks::Access { .. }
            | Asks::Server(_)
            | Asks::Unknown => None,
        }
    }

    /// Whether it hands work to a subagent.
    pub fn hands_off(&self) -> bool {
        matches!(self, Self::Typed(Call::Subagent(_)))
    }

    /// Whether it asks the person something, questions or to approve a plan, which its request for approval carries
    /// in full, so a client draws that rather than the call.
    pub fn asks_the_person(&self) -> bool {
        matches!(self, Self::Typed(Call::Questions | Call::Plan))
    }

    /// What the agent gave a tool with no kind of its own, or `null` for a call of a kind, whose input is its own.
    pub fn input(&self) -> &'a Value {
        match *self {
            Self::Typed(Call::Mcp { input, .. } | Call::Other { input, .. }) => input,
            Self::Typed(_) => &Value::Null,
        }
    }

    /// One line saying what it's about, as [`ToolCall::summary`] says.
    pub fn summary(&self) -> Option<String> {
        ToolCall::read(*self).summary()
    }
}

impl<'a> ToolCall<'a> {
    pub fn read(call: Recorded<'a>) -> Self {
        match call {
            Recorded::Typed(call) => super::typed::read(call),
        }
    }

    /// One line saying what the call is about, the same wherever it's shown, such as where a client asks whether to
    /// run it or in a terminal: the input that matters most for its kind, or for a subagent what it's for. A tool
    /// without a kind of its own has what [`Other`] says. `None` for a call with nothing to say, such as the advisor,
    /// which takes no input.
    pub fn summary(&self) -> Option<String> {
        let about = match self {
            ToolCall::Command(command) => command.summary(),
            ToolCall::Read(ReadFile { path, .. })
            | ToolCall::Write(WriteFile { path, .. })
            | ToolCall::Edit(EditFile { path, .. }) => path,
            ToolCall::Search(search) => search.pattern,
            ToolCall::Agent(agent) => agent.described().or(agent.prompt)?,
            ToolCall::ToSubagents(to) => return to.summary(),
            ToolCall::Fetch(fetch) => fetch.url,
            ToolCall::WebSearch(search) => search.query,
            ToolCall::Advisor => return None,
            ToolCall::Patch(patch) => return Some(super::typed::paths(patch)),
            ToolCall::NotebookEdit(edit) => &edit.path,
            ToolCall::MakeImage(made) => {
                return Some(first_line(&made.prompt)).filter(|about| !about.is_empty()).map(str::to_string);
            }
            ToolCall::Other(other) => return other.summary.clone(),
            ToolCall::Typed(call) => return super::typed::summary(call),
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

/// How many of something there are, in words, such as `1 line` or `1,204 lines`, with the count whole.
pub fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{} {many}", written_counts::whole(n)) }
}

/// The first line of `text` with anything on it, trimmed, which is all a line about a call has room for.
pub fn first_line(text: &str) -> &str {
    text.lines().find(|line| !line.trim().is_empty()).unwrap_or("").trim()
}

pub(super) fn str_field<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}
