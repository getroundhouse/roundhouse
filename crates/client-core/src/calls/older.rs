//! Calls recorded before adapters said what kind each is, which carry only the agent's name for the tool, as the one
//! adapter of then, Claude's, sent it. This reads the kind from that name, and how a failed result says it failed from
//! its text, as Claude wrote both then. It's only for a call or a result with no kind, which an adapter that says
//! kinds never records, so it stays as Claude's tools were then, whatever they become. `agents::claude::kinds` has the
//! same names today, for what's recorded now, and follows the CLI where this doesn't.

use protocol::event::CallKind;

use crate::approval::older as approval;

/// How Claude named a tool from an MCP server: `mcp__<server>__<tool>`.
const MCP_PREFIX: &str = "mcp__";
const MCP_SEPARATOR: &str = "__";

const EXIT_CODE: &str = "Exit code ";
const TOOL_ERROR_OPEN: &str = "<tool_use_error>";
const TOOL_ERROR_CLOSE: &str = "</tool_use_error>";

/// What kind of call one to the tool `name` was.
pub(super) fn kind_of(name: &str) -> CallKind {
    match name {
        "Bash" => CallKind::Command,
        "Read" => CallKind::Read,
        "Write" => CallKind::Write,
        "Edit" | "MultiEdit" => CallKind::Edit,
        "Grep" => CallKind::Search,
        "Glob" => CallKind::FindFiles,
        "WebFetch" => CallKind::Fetch,
        "WebSearch" => CallKind::WebSearch,
        // Claude called it `Agent`, and before that `Task`.
        "Agent" | "Task" => CallKind::Subagent,
        // The API runs it rather than the agent.
        "advisor" => CallKind::Advisor,
        approval::QUESTIONS => CallKind::Questions,
        approval::PLAN => CallKind::Plan,
        _ => match name.strip_prefix(MCP_PREFIX).and_then(|rest| rest.split_once(MCP_SEPARATOR)) {
            Some((server, tool)) => CallKind::Other { server: Some(server.to_string()), tool: tool.to_string() },
            None => CallKind::Other { server: None, tool: name.to_string() },
        },
    }
}

/// How the command exited, which Claude said on the first line of a failed one's result, as `Exit code N`.
pub(super) fn exit_code(text: &str) -> Option<i32> {
    text.lines().next()?.trim().strip_prefix(EXIT_CODE)?.parse().ok()
}

/// What went wrong, without what Claude put around it: the line saying how a command exited, or the tags around a
/// tool's own refusal.
pub(super) fn output(text: &str) -> &str {
    if exit_code(text).is_some() {
        return text.split_once('\n').map_or("", |(_, rest)| rest);
    }
    let trimmed = text.trim();
    if trimmed.starts_with(TOOL_ERROR_OPEN) {
        trimmed.trim_start_matches(TOOL_ERROR_OPEN).trim_end_matches(TOOL_ERROR_CLOSE)
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use protocol::event::CallKind;

    use super::{exit_code, kind_of, output};

    #[test]
    fn each_of_claude_s_tools_of_then_is_the_kind_of_call_it_was() {
        let kinds = [
            ("Bash", CallKind::Command),
            ("Read", CallKind::Read),
            ("Write", CallKind::Write),
            ("Edit", CallKind::Edit),
            ("MultiEdit", CallKind::Edit),
            ("Grep", CallKind::Search),
            ("Glob", CallKind::FindFiles),
            ("WebFetch", CallKind::Fetch),
            ("WebSearch", CallKind::WebSearch),
            ("Agent", CallKind::Subagent),
            ("Task", CallKind::Subagent),
            ("advisor", CallKind::Advisor),
            ("AskUserQuestion", CallKind::Questions),
            ("ExitPlanMode", CallKind::Plan),
        ];
        for (name, kind) in kinds {
            assert_eq!(kind_of(name), kind, "{name}");
        }
    }

    #[test]
    fn any_other_tool_has_no_kind_of_its_own_and_one_from_an_mcp_server_says_which() {
        let from_a_server = CallKind::Other { server: Some("github".into()), tool: "create_issue".into() };
        assert_eq!(kind_of("mcp__github__create_issue"), from_a_server);
        assert_eq!(kind_of("NotebookEdit"), CallKind::Other { server: None, tool: "NotebookEdit".into() });
        assert_eq!(kind_of("mcp__odd"), CallKind::Other { server: None, tool: "mcp__odd".into() });
    }

    #[test]
    fn a_failed_command_s_result_said_how_it_exited_on_its_first_line() {
        let text = "Exit code 2\nls: cannot access 'missing.rs': No such file or directory";

        assert_eq!(exit_code(text), Some(2));
        assert_eq!(output(text), "ls: cannot access 'missing.rs': No such file or directory");
        assert_eq!((exit_code("Exit code 1"), output("Exit code 1")), (Some(1), ""));
    }

    #[test]
    fn output_that_only_looks_like_an_exit_code_is_kept() {
        assert_eq!(exit_code("Exit code soon\nmore"), None);
        assert_eq!(output("Exit code soon\nmore"), "Exit code soon\nmore");
    }

    #[test]
    fn a_tool_s_own_refusal_came_in_tags() {
        let text = "<tool_use_error>regex parse error:\n    Stop(\nerror: unclosed group</tool_use_error>";

        assert_eq!(exit_code(text), None);
        assert_eq!(output(text), "regex parse error:\n    Stop(\nerror: unclosed group", "all of it, without its tags");
    }
}
