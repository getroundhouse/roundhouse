//! Each of Claude's tools as the kind of call it made, read from the tool's name and input as the CLI took them when
//! the adapter stopped recording them untyped. One whose input isn't what its kind takes is any other tool's, with its
//! name and input, rather than dropped.
//!
//! A command says nothing of what it does, since the adapter didn't read commands then, so a chat draws it as it drew
//! it then, as a command, whatever it runs.

use protocol::capability::calls::call::{
    CellEdit, Command, Edit, Fetch, FindFiles, LineRange, Monitor, NotebookEdit, Read, Replace, Replaces, Runs, Search,
    Shows, StartSubagent, UseSkill, WebSearch, Write,
};
use protocol::capability::calls::{Call, CallKind};
use protocol::capability::output::{Todo, TodoList, TodoStatus};
use protocol::capability::setup::ServerName;
use serde_json::Value;

/// The tool that asked the person questions.
pub(super) const QUESTIONS: &str = "AskUserQuestion";
/// The tool that asked to leave plan mode, with the plan.
pub(super) const PLAN: &str = "ExitPlanMode";

/// How the CLI named a tool from an MCP server: `mcp__<server>__<tool>`.
const MCP_PREFIX: &str = "mcp__";
const MCP_SEPARATOR: &str = "__";

/// The kind of call `tool` made, by its name alone, as a rule the CLI suggested named a tool.
pub(super) fn kind(tool: &str) -> CallKind {
    match tool {
        "Bash" => CallKind::Command,
        "Read" => CallKind::Read,
        "Write" => CallKind::Write,
        "Edit" | "MultiEdit" => CallKind::Edit,
        "NotebookEdit" => CallKind::NotebookEdit,
        "Grep" => CallKind::Search,
        "Glob" => CallKind::FindFiles,
        "WebFetch" => CallKind::Fetch,
        "WebSearch" => CallKind::WebSearch,
        "Agent" | "Task" => CallKind::Subagent,
        "TodoWrite" => CallKind::Todos,
        "Skill" => CallKind::Skill,
        "Monitor" => CallKind::Monitor,
        "advisor" => CallKind::Advisor,
        QUESTIONS => CallKind::Questions,
        PLAN => CallKind::Plan,
        _ if tool.strip_prefix(MCP_PREFIX).is_some_and(|rest| rest.contains(MCP_SEPARATOR)) => CallKind::Mcp,
        _ => CallKind::Other,
    }
}

/// The call one to `tool` with `input` made.
pub(super) fn call(tool: &str, input: &Value) -> Call {
    let read = match tool {
        "Bash" => command(input),
        "Read" => text(input, "file_path").map(|path| {
            let lines = number(input, "offset").map(|first| LineRange { first, count: number(input, "limit") });
            Call::Read(Read { path, lines, pages: text(input, "pages") })
        }),
        "Write" => match (text(input, "file_path"), text(input, "content")) {
            (Some(path), Some(content)) => Some(Call::Write(Write { path, content })),
            _ => None,
        },
        "Edit" => text(input, "file_path")
            .zip(replace(input))
            .map(|(path, edit)| Call::Edit(Edit { path, edits: vec![edit] })),
        "MultiEdit" => text(input, "file_path").zip(edits(input)).map(|(path, edits)| Call::Edit(Edit { path, edits })),
        "NotebookEdit" => notebook_edit(input),
        "Grep" => search(input),
        "Glob" => {
            text(input, "pattern").map(|pattern| Call::FindFiles(FindFiles { pattern, path: text(input, "path") }))
        }
        "WebFetch" => text(input, "url").map(|url| Call::Fetch(Fetch { url, asking: text(input, "prompt") })),
        "WebSearch" => web_search(input),
        // Its calls named it `Agent`, and before that `Task`.
        "Agent" | "Task" => subagent(input),
        "TodoWrite" => todos(input),
        "Skill" => {
            text(input, "skill").map(|name| Call::Skill(UseSkill { name: name.into(), arguments: text(input, "args") }))
        }
        "Monitor" => monitor(input),
        // The API ran it rather than the CLI, and it took no input.
        "advisor" => Some(Call::Advisor),
        QUESTIONS => Some(Call::Questions),
        PLAN => Some(Call::Plan),
        _ => mcp(tool, input),
    };
    read.unwrap_or_else(|| Call::Other { tool: tool.to_owned(), input: input.clone() })
}

fn search(input: &Value) -> Option<Call> {
    Some(Call::Search(Search {
        pattern: text(input, "pattern")?,
        path: text(input, "path"),
        files: text(input, "glob").or_else(|| text(input, "type")),
        shows: match input.get("output_mode").and_then(Value::as_str) {
            Some("content") => Shows::Lines,
            Some("count") => Shows::Count,
            // The CLI listed the files that matched unless it was asked for more.
            Some("files_with_matches") | None => Shows::Files,
            Some(_) => Shows::Unknown,
        },
    }))
}

fn web_search(input: &Value) -> Option<Call> {
    Some(Call::WebSearch(WebSearch {
        query: text(input, "query")?,
        only_domains: texts(input, "allowed_domains"),
        not_domains: texts(input, "blocked_domains"),
    }))
}

fn subagent(input: &Value) -> Option<Call> {
    Some(Call::Subagent(StartSubagent {
        persona: text(input, "subagent_type").map(Into::into),
        description: text(input, "description"),
        prompt: text(input, "prompt")?,
        model: None,
        effort: None,
        runs: runs(input),
        resumes: None,
    }))
}

fn monitor(input: &Value) -> Option<Call> {
    Some(Call::Monitor(Monitor {
        command: text(input, "command")?,
        description: text(input, "description"),
        timeout_ms: input.get("timeout_ms").and_then(Value::as_u64),
    }))
}

fn command(input: &Value) -> Option<Call> {
    let command = text(input, "command")?;
    // The CLI asked to run outside the sandbox without saying why.
    let unsandboxed = (input.get("dangerouslyDisableSandbox").and_then(Value::as_bool) == Some(true)).then(String::new);
    Some(Call::Command(Command {
        command,
        cwd: None,
        description: text(input, "description"),
        timeout_ms: input.get("timeout").and_then(Value::as_u64),
        runs: runs(input),
        parts: Vec::new(),
        unsandboxed,
    }))
}

/// Whether it worked on in the background, rather than its call waiting for it.
fn runs(input: &Value) -> Runs {
    if input.get("run_in_background").and_then(Value::as_bool) == Some(true) { Runs::Background } else { Runs::Waits }
}

/// One replacement, as `Edit` and each of `MultiEdit`'s edits said it.
fn replace(edit: &Value) -> Option<Replace> {
    let every = edit.get("replace_all").and_then(Value::as_bool) == Some(true);
    Some(Replace {
        old: text(edit, "old_string")?,
        new: text(edit, "new_string")?,
        replaces: if every { Replaces::Every } else { Replaces::One },
    })
}

fn edits(input: &Value) -> Option<Vec<Replace>> {
    input.get("edits")?.as_array()?.iter().map(replace).collect()
}

fn notebook_edit(input: &Value) -> Option<Call> {
    let does = match input.get("edit_mode").and_then(Value::as_str) {
        Some("replace") | None => CellEdit::Replace,
        Some("insert") => CellEdit::InsertAfter,
        Some("delete") => CellEdit::Delete,
        Some(_) => CellEdit::Unknown,
    };
    Some(Call::NotebookEdit(NotebookEdit {
        path: text(input, "notebook_path")?,
        cell: text(input, "cell_id").map(Into::into),
        source: text(input, "new_source").unwrap_or_default(),
        does,
    }))
}

/// The todo list, whole, as `TodoWrite` gave it each time.
fn todos(input: &Value) -> Option<Call> {
    let items = input.get("todos")?.as_array()?.iter().map(todo).collect::<Option<_>>()?;
    Some(Call::Todos(TodoList { items, note: None }))
}

fn todo(item: &Value) -> Option<Todo> {
    let status = match item.get("status").and_then(Value::as_str)? {
        "pending" => TodoStatus::Pending,
        "in_progress" => TodoStatus::InProgress,
        "completed" => TodoStatus::Done,
        _ => TodoStatus::Unknown,
    };
    Some(Todo {
        id: None,
        text: text(item, "content")?,
        status,
        doing: text(item, "activeForm"),
        priority: None,
        after: Vec::new(),
    })
}

/// A call to a tool from an MCP server, by the server's name and the tool's.
fn mcp(tool: &str, input: &Value) -> Option<Call> {
    let (server, tool) = tool.strip_prefix(MCP_PREFIX)?.split_once(MCP_SEPARATOR)?;
    Some(Call::Mcp { server: ServerName::from(server), tool: tool.to_owned(), input: input.clone() })
}

fn text(input: &Value, key: &str) -> Option<String> {
    input.get(key)?.as_str().map(str::to_owned)
}

fn texts(input: &Value, key: &str) -> Vec<String> {
    let found = input.get(key).and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    found.iter().filter_map(Value::as_str).map(str::to_owned).collect()
}

fn number(input: &Value, key: &str) -> Option<u32> {
    input.get(key)?.as_u64().and_then(|n| u32::try_from(n).ok())
}
