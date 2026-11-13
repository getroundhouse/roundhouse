//! What a tool call is, and what its result says, for every client alike. The agent's adapter says what kind of call
//! each is, and a client reads the call's input knowing its kind, so a client draws kinds of call rather than tools. A
//! call recorded before adapters said is read from its tool's name, as [`older`] has Claude's of then. A tool with no
//! kind of its own, such as one from an MCP server, is `Other`, and still gets a readable line. [`ToolCall::summary`]
//! is the one line every client gives a call to say what it's about.

use std::borrow::Cow;

use protocol::event::CallKind;
use serde_json::Value;

pub mod agent;
mod command;
pub mod failure;
pub mod files;
mod older;
mod other;
mod search;
pub mod web;

pub use agent::Agent;
pub use command::Command;
pub use failure::{Failed, Failure};
pub use files::{EditFile, ReadFile, WriteFile};
pub use other::Other;
pub use search::{FileLines, Found, Looks, Search};
pub use web::{Fetch, Link, Results, WebSearch};

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
}

/// What kind of call one to the tool `name` is: `said`, which is what its adapter said it is, or for one recorded
/// before adapters said, what Claude's tool of that name was.
pub fn kind<'a>(said: Option<&'a CallKind>, name: &str) -> Cow<'a, CallKind> {
    said.map_or_else(|| Cow::Owned(older::kind_of(name)), Cow::Borrowed)
}

/// Whether the call hands work to a subagent.
pub fn hands_off(said: Option<&CallKind>, name: &str) -> bool {
    *kind(said, name) == CallKind::Subagent
}

/// Whether the call asks the person something, questions or to approve a plan, which its request for approval carries
/// in full.
pub fn asks_the_person(said: Option<&CallKind>, name: &str) -> bool {
    matches!(*kind(said, name), CallKind::Questions | CallKind::Plan)
}

impl<'a> ToolCall<'a> {
    /// The call as it's recorded: what its adapter `said` it is, if it said, the agent's own `name` for the tool, and
    /// what it gave it.
    pub fn read(said: Option<&CallKind>, name: &'a str, input: &'a Value) -> Self {
        let kind = kind(said, name);
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

    /// One line saying what the call is about, the same wherever it's shown, such as on a card asking to run it or in
    /// a terminal: the input that matters most for its kind, or for a subagent what it's for. A tool without a kind of
    /// its own has what [`Other`] says. `None` for a call with nothing to say, such as the advisor, which takes no
    /// input.
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
        };
        Some(first_line(about).to_string())
    }
}

/// Inputs that usually say what a call is about, in the order they're looked for.
const MAIN_INPUTS: &[&str] = &["command", "file_path", "path", "pattern", "url", "query", "prompt"];

/// The usual input that says most about what a call is about, on one line, if it has one.
fn main_input(input: &Value) -> Option<&str> {
    MAIN_INPUTS.iter().find_map(|key| str_field(input, key)).map(first_line)
}

/// What the agent said a call is for, in its own words, if it said.
fn description(input: &Value) -> Option<&str> {
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

fn str_field<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests;
