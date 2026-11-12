//! What a tool call is, read from its name and input, and what its result says, for every client alike. Every tool a
//! client draws as a kind of call is named here, so a client draws kinds of call rather than tools. A tool it doesn't
//! know, such as one from an MCP server, is `Other`, and still gets a readable line. [`ToolCall::summary`] is the one
//! line every client gives a call to say what it's about.

use serde_json::Value;

pub mod agent;
mod command;
pub mod failure;
pub mod files;
mod other;
mod search;
pub mod web;

pub use agent::Agent;
pub use command::Command;
pub use failure::Failure;
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

impl<'a> ToolCall<'a> {
    pub fn read(name: &'a str, input: &'a Value) -> Self {
        let known = match name {
            "Bash" => Command::read(input).map(ToolCall::Command),
            "Read" => ReadFile::read(input).map(ToolCall::Read),
            "Write" => WriteFile::read(input).map(ToolCall::Write),
            "Edit" | "MultiEdit" => EditFile::read(input).map(ToolCall::Edit),
            "Grep" => Search::read(input, Looks::InFiles).map(ToolCall::Search),
            "Glob" => Search::read(input, Looks::ForNames).map(ToolCall::Search),
            "WebFetch" => Fetch::read(input).map(ToolCall::Fetch),
            "WebSearch" => WebSearch::read(input).map(ToolCall::WebSearch),
            ADVISOR => Some(ToolCall::Advisor),
            // Always drawn as one, so the subagent's work, which its rows leave to the call, shows somewhere.
            name if agent::hands_off(name) => Some(ToolCall::Agent(Agent::read(input))),
            _ => None,
        };
        // A known tool whose input isn't what it should be is still drawn, as any tool would be.
        known.unwrap_or_else(|| ToolCall::Other(Other::read(name, input)))
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

/// The advisor, which the API runs rather than the agent.
const ADVISOR: &str = "advisor";
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
