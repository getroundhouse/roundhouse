//! Views of typed calls. Legacy tool inputs are decoded only by `recorded`; these read the contract directly.

use std::fmt::Write as _;

use super::{Command, EditFile, ReadFile, ToolCall, WriteFile};
use protocol::capability::calls::call::{CellEdit, CommandPart, FilePatch, NotebookEdit, Patch, PatchChange};
use protocol::capability::calls::output::{FileOutcome, McpContent};
use protocol::capability::calls::{Call, CallOutput};

pub(super) fn read(call: &Call) -> ToolCall<'_> {
    match call {
        Call::Command(command) => ToolCall::Command(Command {
            command: &command.command,
            description: command.description.as_deref(),
            reads: only_reads(command),
        }),
        Call::Read(file) => ToolCall::Read(ReadFile { path: &file.path }),
        Call::Write(file) => ToolCall::Write(WriteFile { path: &file.path, content: &file.content }),
        Call::Edit(file) => ToolCall::Edit(EditFile {
            path: &file.path,
            edits: file.edits.iter().map(|e| (e.old.as_str(), e.new.as_str())).collect(),
        }),
        Call::Patch(patch) => ToolCall::Patch(patch),
        Call::NotebookEdit(edit) => ToolCall::NotebookEdit(edit),
        // No agent records these typed yet, so they're drawn as any typed call until one does.
        Call::ViewImage(_)
        | Call::GenerateImage(_)
        | Call::WriteInput(_)
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
        | Call::Skill(_)
        | Call::Lsp(_)
        | Call::Advisor
        | Call::Sleep(_)
        | Call::Monitor(_)
        | Call::Schedule(_)
        | Call::Workflow(_)
        | Call::Questions
        | Call::Plan
        | Call::Mcp { .. }
        | Call::Other { .. }
        | Call::Unknown => ToolCall::Typed(call),
    }
}

/// The one file a command does nothing but read, as its agent read the command. None for one that reads several files,
/// or does anything else, or whose agent didn't say what it does.
fn only_reads(command: &protocol::capability::calls::call::Command) -> Option<&str> {
    let mut read = command.parts.iter().map(|part| match part {
        CommandPart::Read { path } => Some(path.as_str()),
        CommandPart::List { .. } | CommandPart::Search { .. } | CommandPart::Other => None,
    });
    let first = read.next()??;
    read.all(|path| path == Some(first)).then_some(first)
}

/// What a call is, in words, as the CLI prints it and a request to make it is headed. The chat draws some kinds with
/// titles of their own that say more, such as how many lines a read read.
pub fn title(call: &Call) -> String {
    match call {
        Call::Read(_) => "Read".into(),
        Call::Write(_) => "Write".into(),
        Call::Edit(_) | Call::Patch(_) => "Edit".into(),
        Call::NotebookEdit(edit) => notebook_title(edit).into(),
        Call::ViewImage(_) => "View an image".into(),
        Call::GenerateImage(_) => "Make an image".into(),
        Call::Mcp { tool, .. } | Call::Other { tool, .. } => tool.clone(),
        Call::Unknown => "Unknown tool".into(),
        Call::Command(_)
        | Call::WriteInput(_)
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
        | Call::Skill(_)
        | Call::Lsp(_)
        | Call::Advisor
        | Call::Sleep(_)
        | Call::Monitor(_)
        | Call::Schedule(_)
        | Call::Workflow(_)
        | Call::Questions
        | Call::Plan => format!("{:?}", call.kind()),
    }
}

pub fn summary(call: &Call) -> String {
    match call {
        Call::Patch(patch) => paths(patch),
        Call::Mcp { server, tool, .. } => format!("{server}: {tool}"),
        Call::Other { tool, .. } => tool.clone(),
        Call::Command(_)
        | Call::WriteInput(_)
        | Call::Read(_)
        | Call::Write(_)
        | Call::Edit(_)
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
        | Call::Unknown => title(call),
    }
}

/// What changing a notebook's cell does to it, in words.
pub fn notebook_title(edit: &NotebookEdit) -> &'static str {
    match edit.does {
        CellEdit::Replace => "Edit a cell",
        CellEdit::InsertAfter => "Add a cell",
        CellEdit::Delete => "Delete a cell",
        CellEdit::Unknown => "Edit a notebook",
    }
}

/// What came back for the file `file` of a patch, by its path, once what came back says, as `output` does when it's
/// what the patch changed, file by file.
pub fn outcome<'a>(file: &FilePatch, output: Option<&'a CallOutput>) -> Option<&'a FileOutcome> {
    let Some(CallOutput::Changed { files }) = output else { return None };
    files.iter().find(|outcome| outcome.path == file.path)
}

/// The paths of the files a patch changes, in its order.
pub(super) fn paths(patch: &Patch) -> String {
    patch.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>().join(", ")
}

/// The call's own input, without inventing another agent's tool shape.
pub fn input(call: &Call) -> String {
    if let Call::Patch(patch) = call {
        return patch_input(patch);
    }
    match call {
        Call::Mcp { input, .. } | Call::Other { input, .. } => serde_json::to_string_pretty(input).unwrap_or_default(),
        Call::Command(_)
        | Call::WriteInput(_)
        | Call::Read(_)
        | Call::Write(_)
        | Call::Edit(_)
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
        | Call::Unknown => serde_json::to_string_pretty(call).unwrap_or_default(),
    }
}

fn patch_input(patch: &Patch) -> String {
    patch
        .files
        .iter()
        .map(|file| {
            let body = match &file.change {
                PatchChange::Add { content } => content.lines().fold(String::new(), |mut text, line| {
                    let _ = writeln!(text, "+{line}");
                    text
                }),
                PatchChange::Delete => "Deleted\n".into(),
                PatchChange::Update { move_to, hunks } => {
                    let mut text = move_to.as_ref().map_or_else(String::new, |path| format!("Moved to {path}\n"));
                    for hunk in hunks {
                        let _ = writeln!(text, "@@ {}", hunk.after.as_deref().unwrap_or(""));
                        for line in &hunk.lines {
                            let mark = match line.mark {
                                protocol::event::LineMark::Added => '+',
                                protocol::event::LineMark::Removed => '-',
                                protocol::event::LineMark::Same | protocol::event::LineMark::Unknown => ' ',
                            };
                            let _ = writeln!(text, "{mark}{}", line.text);
                        }
                    }
                    text
                }
                PatchChange::Unknown => "Unknown change\n".into(),
            };
            format!("{}\n{body}", file.path)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Readable output for clients that show a tool's result as text. Rich data stays available alongside this view.
pub fn output(output: &CallOutput) -> String {
    match output {
        CallOutput::Command(command) => command.output.clone(),
        CallOutput::Words { text } | CallOutput::Page { text, .. } => text.clone(),
        CallOutput::Report { report } => report.clone(),
        CallOutput::Lines(lines) => lines.lines.join("\n"),
        CallOutput::Paths(paths) => paths.paths.join("\n"),
        CallOutput::Changed { files } => files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>().join("\n"),
        CallOutput::Mcp { content, structured } => {
            let mut texts: Vec<String> = content
                .iter()
                .filter_map(|c| match c {
                    McpContent::Text { text } => Some(text.clone()),
                    McpContent::Resource { uri, text } => Some(text.clone().unwrap_or_else(|| uri.clone())),
                    McpContent::Image { .. } | McpContent::Other { .. } | McpContent::Unknown => None,
                })
                .collect();
            if let Some(value) = structured {
                texts.push(serde_json::to_string_pretty(value).unwrap_or_default());
            }
            texts.join("\n")
        }
        CallOutput::Image { .. } | CallOutput::Encrypted | CallOutput::Backgrounded { .. } => String::new(),
        CallOutput::Document { pages } => pages.map_or_else(|| "Document".into(), |n| format!("{n} pages")),
        CallOutput::Unknown => "Unknown tool output".into(),
        CallOutput::Found(_) | CallOutput::WebResults { .. } => {
            serde_json::to_string_pretty(output).unwrap_or_default()
        }
    }
}
