//! What kind of call each of the CLI's tools makes, which the adapter says as it records a call, its result, and a
//! request to make it, so that no client needs the CLI's names for its tools. A result holds what came back alone: how
//! a command exited, and the tags the CLI puts around a tool's own refusal, come out of its text.
//!
//! `client_core::calls::older` has these names and these two forms too, as they were before adapters said, for what
//! was recorded then. That copy never changes, and this one follows the CLI, so they aren't one table.

use protocol::event::{CallKind, ContentBlock, EventBody};
use protocol::model::ToolUseId;

/// The tool that asks the person questions.
pub(super) const QUESTIONS: &str = "AskUserQuestion";
/// The tool that asks to leave plan mode, with the plan.
pub(super) const PLAN: &str = "ExitPlanMode";

/// How the CLI names a tool from an MCP server: `mcp__<server>__<tool>`.
const MCP_PREFIX: &str = "mcp__";
const MCP_SEPARATOR: &str = "__";

/// How the CLI starts the result of a command that exited with an error: this, then the code, on a line of its own.
const EXIT_CODE: &str = "Exit code ";
/// What the CLI puts around a tool's own refusal.
const TOOL_ERROR_OPEN: &str = "<tool_use_error>";
const TOOL_ERROR_CLOSE: &str = "</tool_use_error>";

/// What kind of call one to `tool` is. A tool with no kind of its own keeps its name, without its server's.
pub(crate) fn of(tool: &str) -> CallKind {
    match tool {
        "Bash" => CallKind::Command,
        "Read" => CallKind::Read,
        "Write" => CallKind::Write,
        "Edit" | "MultiEdit" => CallKind::Edit,
        "Grep" => CallKind::Search,
        "Glob" => CallKind::FindFiles,
        "WebFetch" => CallKind::Fetch,
        "WebSearch" => CallKind::WebSearch,
        // Its calls name it `Agent`, and named it `Task`.
        "Agent" | "Task" => CallKind::Subagent,
        // The API runs it rather than the CLI.
        "advisor" => CallKind::Advisor,
        QUESTIONS => CallKind::Questions,
        PLAN => CallKind::Plan,
        _ => match tool.strip_prefix(MCP_PREFIX).and_then(|rest| rest.split_once(MCP_SEPARATOR)) {
            Some((server, tool)) => CallKind::Other { server: Some(server.to_string()), tool: tool.to_string() },
            None => CallKind::Other { server: None, tool: tool.to_string() },
        },
    }
}

/// Says what kind of call each call in `body` is, and each result the kind of the call it answers, leaving a failed
/// result's text as what came back alone. `answered` is the tool of each call the line answers, by the call.
///
/// Every result says a kind, so that one with none is only ever one recorded before adapters said. A result whose call
/// isn't among `answered`, as when the CLI answers one from before it was started again, says `Unknown`, and is read
/// as a command's would be, since the CLI says how a command exited whatever this adapter knows of the call.
pub(super) fn say(body: &mut EventBody, answered: &[(ToolUseId, String)]) {
    let (EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. }) = body else { return };
    for block in blocks {
        match block {
            ContentBlock::ToolUse { name, call, .. } => *call = Some(of(name)),
            ContentBlock::ToolResult { tool_use_id, is_error, text, call, exit_code, .. } => {
                let tool = answered.iter().find(|(id, _)| id == tool_use_id).map(|(_, tool)| tool);
                let kind = tool.map_or(CallKind::Unknown, |tool| of(tool));
                if *is_error {
                    if matches!(kind, CallKind::Command | CallKind::Unknown) {
                        *exit_code = take_exit_code(text);
                    }
                    // What a command printed is its own, tags and all.
                    if exit_code.is_none() {
                        take_refusal_tags(text);
                    }
                }
                *call = Some(kind);
            }
            ContentBlock::Text { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::Image { .. }
            | ContentBlock::Unknown => {}
        }
    }
}

/// Takes the line saying how a command exited off the start of what it printed, and returns the code.
fn take_exit_code(text: &mut String) -> Option<i32> {
    let (first, rest) = text.split_once('\n').unwrap_or((text.as_str(), ""));
    let code = first.trim().strip_prefix(EXIT_CODE)?.parse().ok()?;
    *text = rest.to_string();
    Some(code)
}

/// Takes the CLI's tags from around a tool's own refusal, leaving what it said.
fn take_refusal_tags(text: &mut String) {
    let trimmed = text.trim();
    if let Some(said) = trimmed.strip_prefix(TOOL_ERROR_OPEN) {
        *text = said.strip_suffix(TOOL_ERROR_CLOSE).unwrap_or(said).to_string();
    }
}

#[cfg(test)]
mod tests {
    use protocol::event::{CallKind, ContentBlock, EventBody};
    use protocol::fixtures;
    use serde_json::json;

    use super::{of, say};

    /// What `say` makes of one result: its kind, its exit code and its text. `tool` is what the call it answers was
    /// to, or `None` when the adapter doesn't know that call.
    fn said(tool: Option<&str>, is_error: bool, text: &str) -> (Option<CallKind>, Option<i32>, String) {
        let mut body =
            EventBody::AgentUser { parent_tool_use_id: None, blocks: vec![fixtures::tool_result("c", text, is_error)] };
        let answered: Vec<_> = tool.map(|tool| ("c".into(), tool.to_string())).into_iter().collect();
        say(&mut body, &answered);
        let EventBody::AgentUser { blocks, .. } = body else { unreachable!() };
        let [ContentBlock::ToolResult { call, exit_code, text, .. }] = blocks.as_slice() else { unreachable!() };
        (call.clone(), *exit_code, text.clone())
    }

    #[test]
    fn each_of_the_cli_s_tools_is_the_kind_of_call_it_makes() {
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
        for (tool, kind) in kinds {
            assert_eq!(of(tool), kind, "{tool}");
        }
    }

    #[test]
    fn any_other_tool_keeps_its_name_and_one_from_an_mcp_server_says_which() {
        let from_a_server = CallKind::Other { server: Some("github".into()), tool: "create_issue".into() };
        assert_eq!(of("mcp__github__create_issue"), from_a_server);
        assert_eq!(of("NotebookEdit"), CallKind::Other { server: None, tool: "NotebookEdit".into() });
        assert_eq!(of("mcp__team__advisor"), CallKind::Other { server: Some("team".into()), tool: "advisor".into() });
        assert_eq!(of("mcp__odd"), CallKind::Other { server: None, tool: "mcp__odd".into() });
    }

    #[test]
    fn a_call_says_its_kind_as_it_s_recorded() {
        let made = |id: &str, name: &str| fixtures::tool_use(id, name, json!({}));
        let blocks = vec![made("a", "Grep"), made("b", "mcp__github__list")];
        let mut body =
            EventBody::Assistant { message_id: None, parent_tool_use_id: None, model: None, blocks, first_index: None };

        say(&mut body, &[]);

        let EventBody::Assistant { blocks, .. } = body else { unreachable!() };
        let listed = CallKind::Other { server: Some("github".into()), tool: "list".into() };
        let [ContentBlock::ToolUse { call: first, .. }, ContentBlock::ToolUse { call: second, .. }] = blocks.as_slice()
        else {
            unreachable!()
        };
        assert_eq!((first, second), (&Some(CallKind::Search), &Some(listed)));
    }

    #[test]
    fn a_failed_command_s_result_says_how_it_exited_apart_from_what_it_printed() {
        let failed = said(Some("Bash"), true, "Exit code 2\nls: x: No such file\nmore");
        assert_eq!(failed, (Some(CallKind::Command), Some(2), "ls: x: No such file\nmore".to_string()));
        assert_eq!(said(Some("Bash"), true, "Exit code 1"), (Some(CallKind::Command), Some(1), String::new()));

        // Only a failed command's first line is taken for that.
        let printed = "Exit code 2\nprinted by the command";
        assert_eq!(said(Some("Bash"), false, printed), (Some(CallKind::Command), None, printed.to_string()));
        assert_eq!(said(Some("Read"), true, printed), (Some(CallKind::Read), None, printed.to_string()));
        let soon = "Exit code soon\nmore";
        assert_eq!(said(Some("Bash"), true, soon), (Some(CallKind::Command), None, soon.to_string()));
    }

    #[test]
    fn a_tool_s_own_refusal_loses_the_cli_s_tags() {
        let refused = "<tool_use_error>regex parse error:\n    Stop(</tool_use_error>";
        assert_eq!(
            said(Some("Grep"), true, refused),
            (Some(CallKind::Search), None, "regex parse error:\n    Stop(".into())
        );
        assert_eq!(said(Some("Grep"), false, refused).2, refused, "only when it failed");
    }

    /// As when the CLI answers a call made before it was started again, whose tool this adapter never saw. It still
    /// says a kind, so a result with none is only ever one from before adapters said.
    #[test]
    fn a_result_whose_call_isnt_known_says_so_and_is_read_as_a_command_s_would_be() {
        let failed = |text: &str| said(None, true, text);
        let unknown = Some(CallKind::Unknown);

        assert_eq!(
            failed("Exit code 2\nls: x: No such file"),
            (unknown.clone(), Some(2), "ls: x: No such file".into())
        );
        let refused = "<tool_use_error>no such tool</tool_use_error>";
        assert_eq!(failed(refused), (unknown.clone(), None, "no such tool".into()));
        assert_eq!(failed("it broke"), (unknown.clone(), None, "it broke".into()));
        assert_eq!(said(None, false, "a.rs"), (unknown, None, "a.rs".into()), "one that worked says so too");
    }

    /// A result is given the kind of the call it answers, by the call, and not that of another the line answers.
    #[test]
    fn a_result_says_the_kind_of_its_own_call_among_those_a_line_answers() {
        let blocks = vec![fixtures::tool_result("read", "a.rs", false), fixtures::tool_result("gone", "b.rs", false)];
        let mut body = EventBody::AgentUser { parent_tool_use_id: None, blocks };

        say(&mut body, &[("ran".into(), "Bash".to_string()), ("read".into(), "Read".to_string())]);

        let EventBody::AgentUser { blocks, .. } = body else { unreachable!() };
        let [ContentBlock::ToolResult { call: read, .. }, ContentBlock::ToolResult { call: gone, .. }] =
            blocks.as_slice()
        else {
            unreachable!()
        };
        assert_eq!((read, gone), (&Some(CallKind::Read), &Some(CallKind::Unknown)));
    }

    /// The tags are the CLI's only around a tool's own refusal. A command that printed them printed them.
    #[test]
    fn what_a_failed_command_printed_keeps_any_tags_in_it() {
        let printed = "<tool_use_error>printed by the command</tool_use_error>";

        let failed = said(Some("Bash"), true, &format!("Exit code 3\n{printed}"));

        assert_eq!(failed, (Some(CallKind::Command), Some(3), printed.to_string()));
    }
}
