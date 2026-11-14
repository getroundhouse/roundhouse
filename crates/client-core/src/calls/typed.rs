//! Views of typed calls. Legacy tool inputs are decoded only by `recorded`; these read the contract directly.

use std::fmt::Write as _;

use super::{Command, EditFile, ReadFile, ToolCall, WriteFile};
use protocol::capability::calls::{Call, CallOutput, call::PatchChange, output::McpContent};

pub(super) fn read(call: &Call) -> ToolCall<'_> {
    match call {
        Call::Command(command) => {
            ToolCall::Command(Command { command: &command.command, description: command.description.as_deref() })
        }
        Call::Read(file) => ToolCall::Read(ReadFile { path: &file.path }),
        Call::Write(file) => ToolCall::Write(WriteFile { path: &file.path, content: &file.content }),
        Call::Edit(file) => ToolCall::Edit(EditFile {
            path: &file.path,
            edits: file.edits.iter().map(|e| (e.old.as_str(), e.new.as_str())).collect(),
        }),
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
        | Call::Mcp { .. }
        | Call::Other { .. }
        | Call::Unknown => ToolCall::Typed(call),
    }
}

pub fn title(call: &Call) -> String {
    match call {
        Call::Patch(_) => "Patch files".into(),
        Call::Mcp { tool, .. } | Call::Other { tool, .. } => tool.clone(),
        Call::Unknown => "Unknown tool".into(),
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
        | Call::Plan => format!("{:?}", call.kind()),
    }
}

pub fn summary(call: &Call) -> String {
    match call {
        Call::Patch(patch) => patch.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>().join(", "),
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

fn patch_input(patch: &protocol::capability::calls::call::Patch) -> String {
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
        CallOutput::Paths { paths, .. } => paths.join("\n"),
        CallOutput::Changed { files } => files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>().join("\n"),
        CallOutput::Mcp { content, structured } => {
            let mut texts: Vec<String> = content
                .iter()
                .filter_map(|c| match c {
                    McpContent::Text { text } => Some(text.clone()),
                    McpContent::Resource { uri, text } => Some(text.clone().unwrap_or_else(|| uri.clone())),
                    McpContent::Image { .. } | McpContent::Unknown => None,
                })
                .collect();
            if let Some(value) = structured {
                texts.push(serde_json::to_string_pretty(value).unwrap_or_default());
            }
            texts.join("\n")
        }
        CallOutput::Image { .. } => String::new(),
        CallOutput::Document { pages } => pages.map_or_else(|| "Document".into(), |n| format!("{n} pages")),
        CallOutput::Unknown => "Unknown tool output".into(),
        CallOutput::Found(_) | CallOutput::WebResults { .. } => {
            serde_json::to_string_pretty(output).unwrap_or_default()
        }
    }
}
