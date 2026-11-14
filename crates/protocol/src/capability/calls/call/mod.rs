//! What a call does, typed by its kind. Paths are as the agent gave them, relative to the worktree or not.

mod commands;
mod files;
mod images;
mod later;
mod lsp;
mod skills;
mod subagents;
mod typed;
mod web;
mod workflows;

pub use commands::{Command, CommandPart, Monitor, Runs, WriteInput};
pub use files::{
    CellEdit, CellId, Edit, FilePatch, FindFiles, LineRange, NotebookEdit, Patch, PatchChange, PatchHunk, Read,
    Replace, Replaces, Search, Shows, Write,
};
pub use images::{GenerateImage, ViewImage};
pub use later::{Schedule, Sleep, When};
pub use lsp::{Lsp, LspQuestion, Position};
pub use skills::UseSkill;
pub use subagents::{MessageSubagent, StartSubagent, StopSubagent, WaitForSubagents};
pub use typed::Call;
pub use web::{Fetch, FindInPage, WebSearch};
pub use workflows::Workflow;
