//! A tool the transcript has no drawing of its own for, such as one from an MCP server. Its line is the agent's
//! description of the call if it gave one, or the tool's name made readable, then the server it's from and the input
//! that matters most.

use serde_json::Value;

use super::{description, first_line, main_input};

/// How Claude names a tool from an MCP server: `mcp__<server>__<tool>`.
const MCP_PREFIX: &str = "mcp__";
const MCP_SEPARATOR: &str = "__";
/// More than a line has room for, so a huge input isn't copied to be cut off.
const DETAIL_CHARS: usize = 200;

pub(crate) struct Other {
    pub(crate) title: String,
    /// The MCP server it's from.
    pub(crate) server: Option<String>,
    /// The tool's name made readable, when the title is the agent's description instead.
    pub(crate) tool: Option<String>,
    /// The input that matters most, on one line.
    pub(crate) detail: Option<String>,
}

impl Other {
    pub(super) fn read(name: &str, input: &Value) -> Self {
        let (server, tool) = match name.strip_prefix(MCP_PREFIX).and_then(|rest| rest.split_once(MCP_SEPARATOR)) {
            Some((server, tool)) => (Some(server.to_string()), readable(tool)),
            None => (None, readable(name)),
        };
        let (title, tool) = match description(input) {
            Some(description) => (description.to_string(), Some(tool)),
            None => (tool, None),
        };
        Self { title, server, tool, detail: detail(input) }
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

/// The input that matters most: one of the usual ones, or else the first, with its name.
fn detail(input: &Value) -> Option<String> {
    let line = |text: &str| first_line(text).chars().take(DETAIL_CHARS).collect::<String>();
    if let Some(main) = main_input(input) {
        return Some(line(main));
    }
    let (key, value) = input.as_object()?.iter().find(|(key, _)| *key != "description")?;
    let value = match value.as_str() {
        Some(text) => format!("\"{}\"", line(text)),
        None => line(&value.to_string()),
    };
    Some(format!("{key}: {value}"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Other, readable};

    #[test]
    fn a_tool_from_an_mcp_server_says_which_server() {
        let other = Other::read("mcp__github__create_issue", &json!({ "title": "Signals" }));

        assert_eq!(other.title, "Create issue");
        assert_eq!(other.server.as_deref(), Some("github"));
        assert_eq!(other.detail.as_deref(), Some("title: \"Signals\""));
        let other = Other::read("mcp__github__list", &json!({ "labels": ["bug"] }));
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
        let other = Other::read("NotebookEdit", &json!({ "description": "Fix the plot", "notebook_path": "a.ipynb" }));

        assert_eq!(other.title, "Fix the plot");
        assert_eq!(other.tool.as_deref(), Some("Notebook edit"));
        assert_eq!(other.detail.as_deref(), Some("notebook_path: \"a.ipynb\""));
    }

    #[test]
    fn the_usual_input_is_shown_on_its_own() {
        let other = Other::read("ToolSearch", &json!({ "max_results": 2, "query": "select:Read\nmore" }));

        assert_eq!(other.detail.as_deref(), Some("select:Read"));
        assert_eq!(Other::read("Ping", &json!({})).detail, None);
    }
}
