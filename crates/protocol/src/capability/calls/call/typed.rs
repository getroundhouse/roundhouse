//! A call, typed by its kind.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    CellEdit, Command, Edit, Fetch, FindFiles, FindInPage, GenerateImage, Lsp, LspQuestion, MessageSubagent, Monitor,
    NotebookEdit, Patch, Read, Replaces, Runs, Schedule, Search, Shows, Sleep, StartSubagent, StopSubagent, UseSkill,
    ViewImage, WaitForSubagents, WebSearch, When, Workflow, Write, WriteInput,
};
use crate::capability::calls::CallKind;
use crate::capability::output::TodoList;
use crate::capability::settings::Effort;
use crate::capability::setup::ServerName;

/// What a call does, as its adapter read it from the agent's tool and input. One of a kind this build doesn't know
/// reads as `Unknown`, and a client draws it as a call it can say nothing more of.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Call {
    Command(Command),
    /// Writing to a command that's still running in a terminal, as
    /// [`Capability::Terminals`](crate::capability::calls::Capability::Terminals) allows.
    WriteInput(WriteInput),
    Read(Read),
    /// Writing a whole file, which may not exist yet.
    Write(Write),
    /// Replacing parts of one file.
    Edit(Edit),
    /// Changing several files at once, each added, removed, changed or moved, as Codex's and opencode's patches do.
    Patch(Patch),
    NotebookEdit(NotebookEdit),
    /// Looking through what files hold for a pattern.
    Search(Search),
    /// Looking for files by their names.
    FindFiles(FindFiles),
    Fetch(Fetch),
    WebSearch(WebSearch),
    FindInPage(FindInPage),
    /// Writing the agent's todo list, whole.
    Todos(TodoList),
    /// Handing work to a subagent.
    Subagent(StartSubagent),
    MessageSubagent(MessageSubagent),
    WaitForSubagents(WaitForSubagents),
    StopSubagent(StopSubagent),
    /// Looking at an image, which comes back as one.
    ViewImage(ViewImage),
    GenerateImage(GenerateImage),
    Skill(UseSkill),
    /// Asking a language server about the code, as opencode's `lsp` does.
    Lsp(Lsp),
    /// Asking the advisor, a stronger model that reads the whole conversation and advises the agent. It takes no input.
    Advisor,
    Sleep(Sleep),
    /// Watching a command's output in the background, and waking the agent each time it prints, as Claude's `Monitor`
    /// does.
    Monitor(Monitor),
    /// Setting a prompt to send itself later, as Claude's `ScheduleWakeup` and `CronCreate` do.
    Schedule(Schedule),
    /// Running a workflow, a script that runs subagents of its own, in the background, as Claude's `Workflow` does.
    Workflow(Workflow),
    /// Asking the person questions, which its request for approval carries.
    Questions,
    /// Asking the person to approve a plan, which its request for approval carries.
    Plan,
    /// A tool from an MCP server, by the server's name and the tool's own, and what it was given.
    Mcp {
        server: ServerName,
        tool: String,
        input: Value,
    },
    /// A tool with no kind of its own, by the agent's name for it, and what it was given.
    Other {
        tool: String,
        input: Value,
    },
    #[serde(other)]
    Unknown,
}

impl Call {
    pub fn kind(&self) -> CallKind {
        match self {
            Call::Command(_) => CallKind::Command,
            Call::WriteInput(_) => CallKind::WriteInput,
            Call::Read(_) => CallKind::Read,
            Call::Write(_) => CallKind::Write,
            Call::Edit(_) => CallKind::Edit,
            Call::Patch(_) => CallKind::Patch,
            Call::NotebookEdit(_) => CallKind::NotebookEdit,
            Call::Search(_) => CallKind::Search,
            Call::FindFiles(_) => CallKind::FindFiles,
            Call::Fetch(_) => CallKind::Fetch,
            Call::WebSearch(_) => CallKind::WebSearch,
            Call::FindInPage(_) => CallKind::FindInPage,
            Call::Todos(_) => CallKind::Todos,
            Call::Subagent(_) => CallKind::Subagent,
            Call::MessageSubagent(_) => CallKind::MessageSubagent,
            Call::WaitForSubagents(_) => CallKind::WaitForSubagents,
            Call::StopSubagent(_) => CallKind::StopSubagent,
            Call::ViewImage(_) => CallKind::ViewImage,
            Call::GenerateImage(_) => CallKind::GenerateImage,
            Call::Skill(_) => CallKind::Skill,
            Call::Lsp(_) => CallKind::Lsp,
            Call::Advisor => CallKind::Advisor,
            Call::Sleep(_) => CallKind::Sleep,
            Call::Monitor(_) => CallKind::Monitor,
            Call::Schedule(_) => CallKind::Schedule,
            Call::Workflow(_) => CallKind::Workflow,
            Call::Questions => CallKind::Questions,
            Call::Plan => CallKind::Plan,
            Call::Mcp { .. } => CallKind::Mcp,
            Call::Other { .. } => CallKind::Other,
            Call::Unknown => CallKind::Unknown,
        }
    }

    /// Whether it holds nothing this build doesn't know, so an adapter can give it to its agent, as it does the call a
    /// person edited.
    pub fn is_known(&self) -> bool {
        match self {
            Call::Command(command) => command.runs != Runs::Unknown,
            Call::Edit(edit) => edit.edits.iter().all(|replace| replace.replaces != Replaces::Unknown),
            Call::Patch(patch) => patch.files.iter().all(|file| file.change.is_known()),
            Call::NotebookEdit(edit) => edit.does != CellEdit::Unknown,
            Call::Search(search) => search.shows != Shows::Unknown,
            Call::Todos(todos) => todos.is_known(),
            Call::Subagent(subagent) => subagent.runs != Runs::Unknown && subagent.effort != Some(Effort::Unknown),
            Call::Lsp(lsp) => lsp.asks != LspQuestion::Unknown,
            Call::Schedule(schedule) => schedule.when != When::Unknown,
            Call::WriteInput(_)
            | Call::Read(_)
            | Call::Write(_)
            | Call::FindFiles(_)
            | Call::Fetch(_)
            | Call::WebSearch(_)
            | Call::FindInPage(_)
            | Call::MessageSubagent(_)
            | Call::WaitForSubagents(_)
            | Call::StopSubagent(_)
            | Call::ViewImage(_)
            | Call::GenerateImage(_)
            | Call::Skill(_)
            | Call::Advisor
            | Call::Sleep(_)
            | Call::Monitor(_)
            | Call::Workflow(_)
            | Call::Questions
            | Call::Plan
            | Call::Mcp { .. }
            | Call::Other { .. } => true,
            Call::Unknown => false,
        }
    }
}
