use protocol::capability::calls::{
    Call, CallOutput,
    call::{
        Command, Edit, LineRange, MessageSubagent, Read, Replace, Replaces, Runs, Schedule, StartSubagent,
        StopSubagent, WebSearch, When, Workflow, Write,
    },
    output::{CommandOutput, Ended, FileOutcome, Lines, McpContent},
};
use protocol::capability::setup::ServerName;
use protocol::model::ModelName;
use serde_json::Value;

use crate::acp::{ToolCallWire, ToolKindWire, ToolStatusWire};

const MCP_PREFIX: &str = "mcp_";
const COMMAND_MARKER: &str = "\nCommand output:\n";
const NO_OUTPUT_MARKER: &str = "\nNo output from the command.";
const EXIT_CODE_PREFIX: &str = "Command exited with code ";

pub(super) fn translate_call(wire: &ToolCallWire) -> Call {
    let args = wire.args.as_ref().or(wire.raw_input.as_ref()).unwrap_or(&Value::Null);
    if let Some(name) = wire.name.as_deref() {
        if let Some(mcp) = translate_mcp(name, wire.title.as_deref(), args) {
            return mcp;
        }
        return by_name(name, args, wire);
    }
    by_kind(wire, args)
}

fn by_name(name: &str, args: &Value, wire: &ToolCallWire) -> Call {
    match name {
        "run_command" => {
            let given = str_field(args, &["CommandLine", "command"]);
            // What it does is read only from the command it was given, since a title can say it otherwise.
            let parts = given.as_deref().map_or_else(Vec::new, crate::command_parts::of);
            Call::Command(Command {
                command: given.or_else(|| wire.title.clone()).unwrap_or_default(),
                cwd: None,
                description: str_field(args, &["toolSummary", "description"]),
                // `agy` has no time limit for a command. Its `WaitMsBeforeAsync` is how long it waits before it moves
                // the command to the background, as its tool says, and the command goes on.
                timeout_ms: None,
                runs: if args.get("IsDaemon").and_then(Value::as_bool) == Some(true) {
                    Runs::Background
                } else {
                    Runs::Waits
                },
                parts,
                unsandboxed: None,
            })
        }
        "read_file" => match str_field(args, &["Path", "path"]).or_else(|| wire.locations.first().cloned()) {
            Some(path) => Call::Read(Read { path, lines: line_range(args), pages: None }),
            None => other(name, args),
        },
        // One that adds to the end of a file doesn't write it whole, which is what a `Write` says.
        "write_file" if args.get("Append").and_then(Value::as_bool) == Some(true) => other(name, args),
        "write_file" => match (str_field(args, &["Path", "path"]), str_field(args, &["Content", "content"])) {
            (Some(path), Some(content)) => Call::Write(Write { path, content }),
            _ => other(name, args),
        },
        "edit_file" => edit_call(name, args),
        "search_web" => match str_field(args, &["query"]) {
            Some(query) => Call::WebSearch(WebSearch {
                query,
                only_domains: str_field(args, &["domain"]).into_iter().collect(),
                not_domains: Vec::new(),
            }),
            None => other(name, args),
        },
        "invoke_subagent" => Call::Subagent(StartSubagent {
            persona: str_field(args, &["TypeName"]).map(Into::into),
            description: str_field(args, &["Role", "description"]),
            prompt: str_field(args, &["Prompt", "prompt"]).unwrap_or_default(),
            model: str_field(args, &["Model", "model"])
                .filter(|model| model != "inherit")
                .and_then(|model| ModelName::try_from(model).ok()),
            effort: None,
            runs: Runs::Background,
            resumes: None,
        }),
        "schedule" => schedule_call(args),
        "run_workflow" => Call::Workflow(Workflow { name: str_field(args, &["Description", "ScriptPath"]) }),
        "ask_question" => Call::Questions,
        "send_message" => message_call(name, args),
        "manage_subagents" => stop_call(name, args),
        _ => other(name, args),
    }
}

/// A message to the subagent whose conversation `Recipient` names, as `invoke_subagent` gave its ID. One to a name
/// that starts with `@`, such as `@children`, is to several at once, or none, so it's told as it is.
fn message_call(name: &str, args: &Value) -> Call {
    match (str_field(args, &["Recipient"]), str_field(args, &["Message"])) {
        (Some(recipient), Some(message)) if !recipient.starts_with('@') => {
            Call::MessageSubagent(MessageSubagent { subagent: recipient.into(), message })
        }
        _ => other(name, args),
    }
}

/// Stopping the one subagent `kill` names in `ConversationIds`. Listing them, or stopping several, or all, is told as
/// it is.
fn stop_call(name: &str, args: &Value) -> Call {
    let named = args.get("ConversationIds").and_then(Value::as_array).map(Vec::as_slice);
    match (str_field(args, &["Action"]).as_deref(), named) {
        (Some("kill"), Some([Value::String(conversation)])) => {
            Call::StopSubagent(StopSubagent { subagent: conversation.as_str().into() })
        }
        _ => other(name, args),
    }
}

fn edit_call(name: &str, args: &Value) -> Call {
    let path = str_field(args, &["Path", "path"]);
    let old = str_field(args, &["Target", "old_string"]);
    let new = args.get("Replacement").or_else(|| args.get("new_string")).and_then(Value::as_str);
    match (path, old, new) {
        // `agy`'s `edit_file` replaces every place its `Target` is, as its tool says.
        (Some(path), Some(old), Some(new)) => {
            Call::Edit(Edit { path, edits: vec![Replace { old, new: new.to_owned(), replaces: Replaces::Every }] })
        }
        _ => other(name, args),
    }
}

fn schedule_call(args: &Value) -> Call {
    let when = if let Some(seconds) = args.get("DurationSeconds").and_then(Value::as_u64) {
        When::After { seconds }
    } else {
        When::Cron {
            expression: str_field(args, &["CronExpression"]).unwrap_or_default(),
            repeats: args.get("MaxIterations").and_then(Value::as_u64) != Some(1),
        }
    };
    Call::Schedule(Schedule { prompt: str_field(args, &["Prompt"]).unwrap_or_default(), when })
}

fn line_range(args: &Value) -> Option<LineRange> {
    let start = args.get("StartLine").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok());
    let end = args.get("EndLine").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok());
    match (start, end) {
        (Some(first), Some(last)) => {
            Some(LineRange { first, count: last.checked_sub(first).map(|d| d.saturating_add(1)) })
        }
        (Some(first), None) => Some(LineRange { first, count: None }),
        (None, Some(last)) => Some(LineRange { first: 1, count: Some(last) }),
        (None, None) => None,
    }
}

fn by_kind(wire: &ToolCallWire, args: &Value) -> Call {
    let title = wire.title.as_deref().unwrap_or("tool");
    match wire.kind {
        Some(ToolKindWire::Execute) => Call::Command(Command {
            command: wire.title.clone().unwrap_or_default(),
            cwd: None,
            description: None,
            timeout_ms: None,
            runs: Runs::Waits,
            parts: Vec::new(),
            unsandboxed: None,
        }),
        Some(ToolKindWire::Read) => match wire.locations.first() {
            Some(path) => Call::Read(Read { path: path.clone(), lines: None, pages: None }),
            None => other(title, args),
        },
        _ => other(title, args),
    }
}

fn translate_mcp(name: &str, title: Option<&str>, args: &Value) -> Option<Call> {
    let rest = name.strip_prefix(MCP_PREFIX)?;
    let (server, tool) = title
        .and_then(|t| t.split_once(": "))
        .filter(|(srv, tl)| format!("{}_{}", srv.replace('-', "_"), tl.replace('-', "_")) == rest)
        .unwrap_or_else(|| rest.split_once('_').unwrap_or_default());
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    let mut cleaned = args.clone();
    if let Some(obj) = cleaned.as_object_mut() {
        obj.remove("toolSummary");
    }
    Some(Call::Mcp { server: ServerName::from(server), tool: tool.to_owned(), input: cleaned })
}

fn other(tool: &str, args: &Value) -> Call {
    Call::Other { tool: tool.to_owned(), input: args.clone() }
}

fn str_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| value.get(*key).and_then(Value::as_str).map(str::to_owned))
}

pub(super) fn translate_output(call: &Call, wire: &ToolCallWire, cancelling: bool, was_denied: bool) -> CallOutput {
    let raw = wire.raw_output.as_ref().unwrap_or(&Value::Null);
    match call {
        Call::Command(_) => command_output(raw, wire, cancelling, was_denied),
        Call::Read(_) => {
            let text = raw_text(raw, wire);
            parse_read_lines(&text).map_or(CallOutput::Words { text }, CallOutput::Lines)
        }
        Call::Write(write) => changed_file(&write.path),
        Call::Edit(edit) => changed_file(&edit.path),
        Call::Mcp { .. } => {
            CallOutput::Mcp { content: vec![McpContent::Text { text: raw_text(raw, wire) }], structured: None }
        }
        Call::WriteInput(_)
        | Call::Patch(_)
        | Call::NotebookEdit(_)
        | Call::Search(_)
        | Call::FindFiles(_)
        | Call::Fetch(_)
        | Call::WebSearch(_)
        | Call::FindInPage(_)
        | Call::Todos(_)
        | Call::Subagent(_)
        | Call::MessageSubagent(_)
        | Call::WaitForSubagents(_)
        | Call::StopSubagent(_)
        | Call::ViewImage(_)
        | Call::GenerateImage(_)
        | Call::Skill(_)
        | Call::Lsp(_)
        | Call::Advisor
        | Call::Sleep(_)
        | Call::Monitor(_)
        | Call::Schedule(_)
        | Call::Workflow(_)
        | Call::Questions
        | Call::Plan
        | Call::Other { .. }
        | Call::Unknown => CallOutput::Words { text: raw_text(raw, wire) },
    }
}

fn command_output(raw: &Value, wire: &ToolCallWire, cancelling: bool, was_denied: bool) -> CallOutput {
    let explicit_text =
        raw.get("output").and_then(Value::as_str).or_else(|| raw.as_str()).or(wire.content_text.as_deref());
    let fallback;
    let raw_out = match explicit_text {
        Some(text) => text,
        None if raw.is_null() => "",
        None => {
            fallback = raw.to_string();
            &fallback
        }
    };
    let (output, parsed_exit) = parse_command_envelope(raw_out);
    let exit_code = raw.get("exit_code").and_then(Value::as_i64).and_then(|c| i32::try_from(c).ok()).or(parsed_exit);
    // `agy` reports both `session/cancel` and `manage_task` (`Action: "kill"`) as `status: "failed"` with
    // `rawOutput: {"output": ""}` and no `exit_code`, whereas commands that ran to an exit carry `exit_code`
    // and spawn or validation failures carry error text.
    let stopped = wire.status == Some(ToolStatusWire::Failed)
        && exit_code.is_none()
        && !was_denied
        && (cancelling || (explicit_text.is_some() && output.is_empty()));
    let ended = if stopped { Ended::Stopped } else { Ended::Exited };
    CallOutput::Command(CommandOutput { output, exit_code, ended, took_ms: None, truncated: None })
}

fn changed_file(path: &str) -> CallOutput {
    CallOutput::Changed {
        files: vec![FileOutcome { path: path.to_owned(), change: None, moved_to: None, diagnostics: Vec::new() }],
    }
}

fn raw_text(raw: &Value, wire: &ToolCallWire) -> String {
    if let Some(text) = wire.content_text.as_deref() {
        return text.to_owned();
    }
    if let Some(s) = raw.as_str() {
        return s.to_owned();
    }
    if let Some(s) = raw.get("output").and_then(Value::as_str) {
        return s.to_owned();
    }
    if raw.is_null() { String::new() } else { raw.to_string() }
}

fn parse_read_lines(raw: &str) -> Option<Lines> {
    let mut iter = raw.lines().peekable();
    while iter.peek().is_some_and(|l| l.starts_with("Created At:") || l.starts_with("Completed At:")) {
        iter.next();
    }
    let header = iter.next()?.strip_prefix("Showing lines ")?;
    let (range, total) = header.split_once(" of ")?;
    let (first, _) = range.split_once('-')?;
    let first: u32 = first.parse().ok()?;
    let total: u32 = total.parse().ok()?;
    let lines = iter
        .map(|line| match line.split_once(": ") {
            Some((num, rest)) if num.chars().all(|c| c.is_ascii_digit()) => rest.to_owned(),
            _ => line.to_owned(),
        })
        .collect();
    Some(Lines { first, lines, total: Some(total) })
}

/// Strips `agy`'s `Created At: …\nCompleted At: …\nCommand exited with code N.\n\nCommand output:\n` envelope (with or
/// without timestamp lines) so the transcript shows only what the command printed and recovers its exit code.
fn parse_command_envelope(raw: &str) -> (String, Option<i32>) {
    if !raw.starts_with("Created At:") && !raw.starts_with(EXIT_CODE_PREFIX) {
        return (raw.to_owned(), None);
    }
    let header = raw.split_once(COMMAND_MARKER).map_or(raw, |(head, _)| head);
    let exit_code = header
        .lines()
        .find_map(|line| line.strip_prefix(EXIT_CODE_PREFIX)?.strip_suffix('.'))
        .and_then(|digits| digits.parse::<i32>().ok());
    if let Some((_, body)) = raw.split_once(COMMAND_MARKER) {
        return (body.to_owned(), exit_code);
    }
    if raw.contains(NO_OUTPUT_MARKER) {
        return (String::new(), exit_code);
    }
    (raw.to_owned(), exit_code)
}
