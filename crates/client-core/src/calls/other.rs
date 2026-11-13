//! A tool with no kind of its own, such as one from an MCP server. Its line is the agent's description of the call if
//! it gave one, or the tool's name made readable, then the server it's from and the input that matters most.

use protocol::event::CallKind;
use serde_json::Value;

use super::{description, first_line, main_input};

/// More than a line has room for, so a huge input isn't copied to be cut off.
const DETAIL_CHARS: usize = 200;

pub struct Other {
    pub title: String,
    /// The MCP server it's from.
    pub server: Option<String>,
    /// The tool's name made readable, when the title is the agent's description instead.
    pub tool: Option<String>,
    /// The input that matters most, on one line.
    pub detail: Option<String>,
    /// What [`super::ToolCall::summary`] says of it: the usual input, if it has one, or else what the agent said it's
    /// for, which says more than whichever input comes first.
    pub(super) summary: Option<String>,
}

impl Other {
    /// The tool's name made readable, whichever of the title and `tool` holds it.
    pub fn name(&self) -> &str {
        self.tool.as_deref().unwrap_or(&self.title)
    }

    /// A call of `kind`, to the tool the agent calls `name`. One whose kind says the tool, and the server it's from,
    /// is named by those. Any other, such as a known kind given input it doesn't expect, is named as the agent names
    /// it.
    pub(super) fn read(kind: &CallKind, name: &str, input: &Value) -> Self {
        let (server, tool) = match kind {
            CallKind::Other { server, tool } if !tool.is_empty() => (server.clone(), readable(tool)),
            CallKind::Other { .. }
            | CallKind::Command
            | CallKind::Read
            | CallKind::Write
            | CallKind::Edit
            | CallKind::Search
            | CallKind::FindFiles
            | CallKind::Fetch
            | CallKind::WebSearch
            | CallKind::Subagent
            | CallKind::Advisor
            | CallKind::Questions
            | CallKind::Plan
            | CallKind::Unknown => (None, readable(name)),
        };
        let described = description(input);
        let (title, tool) = match described {
            Some(description) => (description.to_string(), Some(tool)),
            None => (tool, None),
        };
        let usual = main_input(input).map(line);
        let detail = usual.clone().or_else(|| first_input(input));
        let summary = usual.or_else(|| described.map(str::to_string)).or_else(|| detail.clone());
        Self { title, server, tool, detail, summary }
    }
}

/// `GetWorkspaceDiff`, `get_workspace_diff` and `get-workspace-diff` all read "Get workspace diff". A word in
/// capitals, such as `PR`, stays in capitals.
fn readable(name: &str) -> String {
    let words: Vec<String> = name.split(['_', '-', ' ', '.']).flat_map(split_case).collect();
    let mut out = String::new();
    for (i, word) in words.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let shouting = word.chars().filter(|c| c.is_alphabetic()).count() > 1 && !word.chars().any(char::is_lowercase);
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            if shouting {
                out.push_str(word);
            } else if i == 0 {
                out.extend(first.to_uppercase());
                out.push_str(chars.as_str());
            } else {
                out.extend(first.to_lowercase());
                out.push_str(chars.as_str());
            }
        }
    }
    out
}

/// Splits `getWorkspaceDiff` into `get`, `Workspace` and `Diff`, and `HTTPServer` into `HTTP` and `Server`.
fn split_case(part: &str) -> Vec<String> {
    let chars: Vec<char> = part.chars().collect();
    let mut words = Vec::new();
    let mut word = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let after_lower = i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit());
        let ends_capitals = i > 0 && chars[i - 1].is_uppercase() && chars.get(i + 1).is_some_and(|n| n.is_lowercase());
        if c.is_uppercase() && (after_lower || ends_capitals) && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        word.push(c);
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// `text` on one line, and no more of it than a line has room for.
fn line(text: &str) -> String {
    first_line(text).chars().take(DETAIL_CHARS).collect()
}

/// The first input, with its name, for a tool with none of the usual ones.
fn first_input(input: &Value) -> Option<String> {
    let (key, value) = input.as_object()?.iter().find(|(key, _)| *key != "description")?;
    let value = match value.as_str() {
        Some(text) => format!("\"{}\"", line(text)),
        None => line(&value.to_string()),
    };
    Some(format!("{key}: {value}"))
}

#[cfg(test)]
mod tests {
    use protocol::event::CallKind;
    use serde_json::{Value, json};

    use super::{Other, readable};

    /// A call to `tool`, which its kind says is a tool with no kind of its own, and from no server.
    fn read(tool: &str, input: &Value) -> Other {
        Other::read(&CallKind::Other { server: None, tool: tool.to_string() }, tool, input)
    }

    #[test]
    fn a_tool_from_an_mcp_server_says_which_server() {
        let kind = CallKind::Other { server: Some("github".into()), tool: "create_issue".into() };
        let other = Other::read(&kind, "mcp__github__create_issue", &json!({ "title": "Signals" }));

        assert_eq!(other.title, "Create issue");
        assert_eq!(other.server.as_deref(), Some("github"));
        assert_eq!(other.detail.as_deref(), Some("title: \"Signals\""));
        let other = read("list", &json!({ "labels": ["bug"] }));
        assert_eq!(other.detail.as_deref(), Some("labels: [\"bug\"]"));
    }

    #[test]
    fn names_are_made_readable_whatever_their_case() {
        assert_eq!(readable("GetWorkspaceDiff"), "Get workspace diff");
        assert_eq!(readable("get_workspace_diff"), "Get workspace diff");
        assert_eq!(readable("list-open-PR"), "List open PR");
        assert_eq!(readable("HTTPServer"), "HTTP server");
        assert_eq!(readable("WebFetch"), "Web fetch");
    }

    #[test]
    fn the_agent_s_description_comes_first_with_the_tool_after_it() {
        let other = read("NotebookEdit", &json!({ "description": "Fix the plot", "notebook_path": "a.ipynb" }));

        assert_eq!(other.title, "Fix the plot");
        assert_eq!(other.tool.as_deref(), Some("Notebook edit"));
        assert_eq!(other.detail.as_deref(), Some("notebook_path: \"a.ipynb\""));
    }

    #[test]
    fn the_usual_input_is_shown_on_its_own() {
        let other = read("ToolSearch", &json!({ "max_results": 2, "query": "select:Read\nmore" }));

        assert_eq!(other.detail.as_deref(), Some("select:Read"));
        assert_eq!(read("Ping", &json!({})).detail, None);
    }

    /// As a command given no command is, or a call of a kind this build doesn't know.
    #[test]
    fn a_call_whose_kind_names_no_tool_is_named_as_the_agent_names_it() {
        for kind in [CallKind::Command, CallKind::Unknown, CallKind::Other { server: None, tool: String::new() }] {
            let other = Other::read(&kind, "run_script", &json!({ "script": "ls" }));
            assert_eq!((other.title.as_str(), other.server), ("Run script", None), "{kind:?}");
        }
    }
}
