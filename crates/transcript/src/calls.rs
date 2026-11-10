//! What a tool call is, read from its name and input, and what its result says. Every tool the transcript draws as a
//! call is named here, so the rest of it draws kinds of call rather than tools. A tool it doesn't know, such as one
//! from an MCP server, is `Other`, and still gets a readable line. The two that ask the person something are drawn as
//! cards instead, by `questions` and `rows::approval`, and [`asks_in_a_card`] says which they are.

use serde_json::Value;

pub(crate) mod agent;
mod command;
pub(crate) mod diff;
pub(crate) mod failure;
pub(crate) mod files;
pub(crate) mod lines;
mod other;
mod search;
pub(crate) mod web;

pub(crate) use agent::Agent;
pub(crate) use command::Command;
pub(crate) use failure::Failure;
pub(crate) use files::{EditFile, ReadFile, WriteFile};
pub(crate) use other::Other;
pub(crate) use search::{Found, Looks, Search};
pub(crate) use web::{Fetch, WebSearch};

/// A call, by what kind of thing it does.
pub(crate) enum ToolCall<'a> {
    Command(Command<'a>),
    Read(ReadFile<'a>),
    Write(WriteFile<'a>),
    Edit(EditFile<'a>),
    Search(Search<'a>),
    Agent(Agent<'a>),
    Fetch(Fetch<'a>),
    WebSearch(WebSearch<'a>),
    Other(Other),
}

impl<'a> ToolCall<'a> {
    pub(crate) fn read(name: &'a str, input: &'a Value) -> Self {
        let known = match name {
            "Bash" => Command::read(input).map(ToolCall::Command),
            "Read" => ReadFile::read(input).map(ToolCall::Read),
            "Write" => WriteFile::read(input).map(ToolCall::Write),
            "Edit" | "MultiEdit" => EditFile::read(input).map(ToolCall::Edit),
            "Grep" => Search::read(input, Looks::InFiles).map(ToolCall::Search),
            "Glob" => Search::read(input, Looks::ForNames).map(ToolCall::Search),
            "WebFetch" => Fetch::read(input).map(ToolCall::Fetch),
            "WebSearch" => WebSearch::read(input).map(ToolCall::WebSearch),
            // Always drawn as one, so the subagent's work, which shows under the call, has a line that opens to it.
            name if agent::hands_off(name) => Some(ToolCall::Agent(Agent::read(input))),
            _ => None,
        };
        // A known tool whose input isn't what it should be is still drawn, as any tool would be.
        known.unwrap_or_else(|| ToolCall::Other(Other::read(name, input)))
    }
}

/// The tool that asks to leave plan mode, with the plan.
pub(crate) const PLAN: &str = "ExitPlanMode";
/// Inputs that usually say what a call is about, in the order they're looked for.
const MAIN_INPUTS: &[&str] = &["command", "file_path", "path", "pattern", "url", "query", "prompt"];

/// Whether a call to `name` asks the person something, and so shows as a card with everything in it rather than as
/// a call.
pub(crate) fn asks_in_a_card(name: &str) -> bool {
    name == crate::questions::TOOL_NAME || name == PLAN
}

/// One line saying what a tool is doing, for a card that asks to run it: the input that matters most, or else what
/// the agent said it's for.
pub(crate) fn summarize_input(input: &Value) -> String {
    let summary = main_input(input).or_else(|| description(input));
    summary.map_or_else(|| first_line(&input.to_string()).to_string(), str::to_string)
}

/// The usual input that says most about what a call is about, on one line, if it has one.
fn main_input(input: &Value) -> Option<&str> {
    MAIN_INPUTS.iter().find_map(|key| str_field(input, key)).map(first_line)
}

/// What the agent said a call is for, in its own words, if it said.
fn description(input: &Value) -> Option<&str> {
    str_field(input, "description").map(str::trim).filter(|d| !d.is_empty())
}

/// How many of something there are, in words, such as `1 line` or `3 lines`.
pub(crate) fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

/// The first line of `text`, which is all a call's line has room for.
pub(crate) fn first_line(text: &str) -> &str {
    text.lines().find(|line| !line.trim().is_empty()).unwrap_or("").trim()
}

fn str_field<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ToolCall, count, summarize_input};

    #[test]
    fn a_known_tool_with_input_it_does_not_expect_is_drawn_like_any_other() {
        let input = json!({ "script": "ls" });

        let call = ToolCall::read("Bash", &input);

        let ToolCall::Other(other) = call else { panic!("a command needs its command") };
        assert_eq!(other.title, "Bash");
    }

    #[test]
    fn tool_summaries_pick_the_field_that_matters() {
        assert_eq!(summarize_input(&json!({"command": "cargo test\nsecond line", "description": "x"})), "cargo test");
        assert_eq!(summarize_input(&json!({"file_path": "src/a.rs", "old_string": "y"})), "src/a.rs");
        assert_eq!(summarize_input(&json!({"description": " Tidy up ", "todos": []})), "Tidy up");
        assert_eq!(summarize_input(&json!({"other": 1})), "{\"other\":1}");
    }

    #[test]
    fn a_count_says_one_or_many() {
        assert_eq!(count(1, "line", "lines"), "1 line");
        assert_eq!(count(0, "line", "lines"), "0 lines");
        assert_eq!(count(12, "match", "matches"), "12 matches");
    }

    #[test]
    fn a_command_is_known_by_its_tool() {
        let input = json!({ "command": "ls", "description": "List files" });

        assert!(matches!(ToolCall::read("Bash", &input), ToolCall::Command(_)));
        assert!(matches!(ToolCall::read("mcp__shell__run", &input), ToolCall::Other(_)));
    }
}
