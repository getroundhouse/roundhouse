//! A kind of call, without what it was given.

use serde::{Deserialize, Serialize};

/// A kind of call, without what it was given. [`Call::kind`](super::Call::kind) is a call's.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CallKind {
    Command,
    WriteInput,
    Read,
    Write,
    Edit,
    Patch,
    NotebookEdit,
    Search,
    FindFiles,
    Fetch,
    WebSearch,
    FindInPage,
    Todos,
    Subagent,
    MessageSubagent,
    WaitForSubagents,
    StopSubagent,
    ViewImage,
    GenerateImage,
    Skill,
    Lsp,
    Advisor,
    Sleep,
    Monitor,
    Schedule,
    Workflow,
    Questions,
    Plan,
    Mcp,
    Other,
    #[serde(other)]
    Unknown,
}
