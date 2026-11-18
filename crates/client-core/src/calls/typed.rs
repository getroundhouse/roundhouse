//! Views of typed calls, each read from its call's contract.

use std::fmt::Write as _;

use super::agent::Agent;
use super::command::Only;
use super::other::{self, Other};
use super::search::{Looks, Search};
use super::subagents::ToSubagents;
use super::web::{Fetch, WebSearch};
use super::{Command, EditFile, ReadFile, Recorded, ToolCall, WriteFile, count, first_line, plain};
use protocol::capability::calls::call::{CellEdit, CommandPart, FilePatch, NotebookEdit, Patch, PatchChange};
use protocol::capability::calls::output::{FileOutcome, Found, McpContent};
use protocol::capability::calls::{Call, CallOutput};

pub(super) fn read(call: &Call) -> ToolCall<'_> {
    match call {
        Call::Command(command) => ToolCall::Command(Command {
            command: &command.command,
            // What says nothing of what it's for isn't its title.
            description: command.description.as_deref().map(str::trim).filter(|said| !said.is_empty()),
            only: only(command),
        }),
        Call::Read(file) => ToolCall::Read(ReadFile { path: &file.path, lines: file.lines.as_ref() }),
        // A look at an image is a read of it, which draws it.
        Call::ViewImage(view) => ToolCall::Read(ReadFile { path: &view.path, lines: None }),
        Call::GenerateImage(made) => ToolCall::MakeImage(made),
        Call::Write(file) => ToolCall::Write(WriteFile { path: &file.path, content: &file.content }),
        Call::Edit(file) => ToolCall::Edit(EditFile {
            path: &file.path,
            edits: file.edits.iter().map(|e| (e.old.as_str(), e.new.as_str())).collect(),
        }),
        Call::Patch(patch) => ToolCall::Patch(patch),
        Call::NotebookEdit(edit) => ToolCall::NotebookEdit(edit),
        Call::Search(search) => {
            ToolCall::Search(Search { pattern: &search.pattern, path: search.path.as_deref(), looks: Looks::InFiles })
        }
        Call::FindFiles(find) => {
            ToolCall::Search(Search { pattern: &find.pattern, path: find.path.as_deref(), looks: Looks::ForNames })
        }
        Call::Fetch(fetch) => ToolCall::Fetch(Fetch { url: &fetch.url, prompt: fetch.asking.as_deref() }),
        Call::WebSearch(search) => ToolCall::WebSearch(WebSearch { query: &search.query }),
        Call::Subagent(start) => ToolCall::Agent(Agent::typed(start)),
        Call::MessageSubagent(to) => ToolCall::ToSubagents(to.into()),
        Call::WaitForSubagents(wait) => ToolCall::ToSubagents(wait.into()),
        Call::StopSubagent(stop) => ToolCall::ToSubagents(stop.into()),
        Call::Mcp { server, tool, input } => ToolCall::Other(Other::typed(Some(server.as_str()), tool, input)),
        Call::Other { tool, input } => ToolCall::Other(Other::typed(None, tool, input)),
        Call::Advisor => ToolCall::Advisor,
        // Drawn as any typed call until each is drawn as its kind.
        Call::WriteInput(_)
        | Call::FindInPage(_)
        | Call::Todos(_)
        | Call::Skill(_)
        | Call::Lsp(_)
        | Call::Sleep(_)
        | Call::Monitor(_)
        | Call::Schedule(_)
        | Call::Workflow(_)
        | Call::Questions
        | Call::Plan
        | Call::Unknown => ToolCall::Typed(call),
    }
}

/// All a command does, as its agent read the command, where that's one kind of call: reading one file, or one search
/// of what's in files, for a pattern it says. `None` for one that reads several files or searches several ways, or does
/// anything else, or whose agent didn't say what it does.
fn only(command: &protocol::capability::calls::call::Command) -> Option<Only<'_>> {
    let mut does = command.parts.iter().map(|part| match part {
        CommandPart::Read { path } => Some(Only::Reads(path)),
        CommandPart::Search { pattern: Some(pattern), path } => {
            Some(Only::Searches(Search { pattern, path: path.as_deref(), looks: Looks::InFiles }))
        }
        CommandPart::Search { pattern: None, .. } | CommandPart::List { .. } | CommandPart::Other => None,
    });
    let first = does.next()??;
    does.all(|part| part == Some(first)).then_some(first)
}

/// What a call is, in words, as the CLI prints it and a request to make it is headed. The chat draws some kinds with
/// titles of their own that say more, such as how many lines a read read.
pub fn title(call: &Call) -> String {
    let words = match call {
        Call::Command(_) => "Command",
        Call::WriteInput(input) => plain::write_input_title(input),
        Call::Read(_) => "Read",
        Call::Write(_) => "Write",
        Call::Edit(_) | Call::Patch(_) => "Edit",
        Call::NotebookEdit(edit) => notebook_title(edit),
        Call::Search(_) => Looks::InFiles.title(),
        Call::FindFiles(_) => Looks::ForNames.title(),
        Call::Fetch(_) => Fetch::TITLE,
        Call::WebSearch(_) => WebSearch::TITLE,
        Call::FindInPage(_) => "Find in a page",
        Call::Todos(_) => "Update the todo list",
        Call::Subagent(_) => "Subagent",
        Call::MessageSubagent(to) => ToSubagents::from(to).title(),
        Call::WaitForSubagents(wait) => ToSubagents::from(wait).title(),
        Call::StopSubagent(stop) => ToSubagents::from(stop).title(),
        Call::ViewImage(_) => "View an image",
        Call::GenerateImage(_) => "Make an image",
        Call::Skill(_) => "Use a skill",
        Call::Lsp(lsp) => plain::lsp_title(lsp),
        Call::Advisor => "Ask the advisor",
        Call::Sleep(_) => "Wait",
        Call::Monitor(_) => "Monitor",
        Call::Schedule(_) => "Schedule a prompt",
        Call::Workflow(_) => "Run a workflow",
        Call::Questions => "Ask questions",
        Call::Plan => "Ask to approve a plan",
        Call::Mcp { tool, .. } | Call::Other { tool, .. } => return tool.clone(),
        Call::Unknown => other::UNKNOWN_TOOL,
    };
    words.into()
}

/// What a call read as any typed call is about, on one line, as [`ToolCall::summary`] says: the input that says most
/// of what it does, as any other tool's is. `None` for one with nothing to say, such as a wait for any subagent.
pub(super) fn summary(call: &Call) -> Option<String> {
    let about = match call {
        Call::WriteInput(input) => Some(input.text.as_str()),
        Call::FindInPage(find) => Some(find.pattern.as_str()),
        Call::Todos(list) => return Some(count(list.items.len(), "todo", "todos")),
        Call::Skill(skill) => Some(skill.name.as_str()),
        Call::Lsp(lsp) => return Some(plain::lsp_place(lsp)),
        Call::Sleep(sleep) => return Some(plain::how_long(*sleep)),
        Call::Monitor(monitor) => return Some(plain::watching(monitor)),
        Call::Schedule(schedule) => Some(schedule.prompt.as_str()),
        Call::Workflow(workflow) => workflow.name.as_deref(),
        Call::Advisor | Call::Questions | Call::Plan | Call::Unknown => None,
        // Read as kinds of their own, which say what they're about themselves.
        Call::Command(_)
        | Call::Read(_)
        | Call::Write(_)
        | Call::Edit(_)
        | Call::Patch(_)
        | Call::NotebookEdit(_)
        | Call::Search(_)
        | Call::FindFiles(_)
        | Call::Fetch(_)
        | Call::WebSearch(_)
        | Call::ViewImage(_)
        | Call::GenerateImage(_)
        | Call::Subagent(_)
        | Call::MessageSubagent(_)
        | Call::WaitForSubagents(_)
        | Call::StopSubagent(_)
        | Call::Mcp { .. }
        | Call::Other { .. } => return ToolCall::read(Recorded::Typed(call)).summary(),
    };
    about.map(first_line).filter(|about| !about.is_empty()).map(str::to_string)
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
        // An image says what it was made from, if it's one the agent made.
        CallOutput::Image { prompt, .. } => prompt.clone().unwrap_or_default(),
        CallOutput::Encrypted | CallOutput::Backgrounded { .. } => String::new(),
        CallOutput::Document { pages } => pages.map_or_else(|| "Document".into(), |n| format!("{n} pages")),
        CallOutput::Unknown => "Unknown tool output".into(),
        CallOutput::Found(found) => found_text(found),
        CallOutput::WebResults { results, summary } => {
            let links = results.iter().map(|result| format!("{} ({})", result.title, result.url));
            let links = links.collect::<Vec<_>>().join("\n");
            [links.as_str(), summary.as_deref().unwrap_or_default()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n")
        }
    }
}

/// What a search found, as ripgrep prints it: each matching line as `path:number:text`, or the files, one to a line.
fn found_text(found: &Found) -> String {
    match found {
        Found::Lines { lines, .. } => {
            lines.iter().map(|line| format!("{}:{}:{}", line.path, line.line, line.text)).collect::<Vec<_>>().join("\n")
        }
        Found::Files(paths) => paths.paths.join("\n"),
        Found::Count { matches, files } => super::Found::Count { matches: *matches, files: *files }.summary(),
        Found::Unknown => "Unknown search result".into(),
    }
}
