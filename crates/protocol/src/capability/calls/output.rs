//! What a call sent back, typed by its kind, in place of the words the agent was told.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::attachment::Attachment;
use crate::event::FileChange;
use crate::model::TaskId;

/// What a call sent back, as its adapter read it, which is what a client draws. The words the model read are the raw
/// line's, or [`CallOutput::Words`] for a kind with nothing more to say. One this build doesn't know reads as
/// `Unknown`, and a client says it can't show it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CallOutput {
    Command(CommandOutput),
    /// A file's lines, from a read.
    Lines(Lines),
    /// An image, from a read, a look or one the agent made.
    Image {
        image: Attachment,
        /// The prompt it was made from, as the model rewrote it, for one it made.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
    },
    /// A document, from a read, which the model reads whole.
    Document {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pages: Option<u32>,
    },
    /// What a write, an edit or a patch did, file by file, in the order the call named them.
    Changed {
        files: Vec<FileOutcome>,
    },
    /// What a search found.
    Found(Found),
    /// The paths a search for files found.
    Paths {
        paths: Vec<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        truncated: bool,
    },
    /// A page, as the tool sent it back: the page itself, or what the tool's model said of it.
    Page {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<u16>,
        text: String,
    },
    WebResults {
        results: Vec<WebResult>,
    },
    /// What a subagent reported, once it finished.
    Report {
        report: String,
    },
    /// What an MCP tool sent back, as its blocks, and the structured result it gave beside them.
    Mcp {
        content: Vec<McpContent>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        structured: Option<Value>,
    },
    /// Nothing but words, as the agent was told them.
    Words {
        text: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CommandOutput {
    /// What it printed, its errors among the rest in the order they came, as far as the agent kept it.
    pub output: String,
    /// `None` where the agent doesn't say, or the command hasn't ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(flatten)]
    pub ended: Ended,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub took_ms: Option<u64>,
    /// Where the rest of what it printed is, when the agent kept only part, in a file of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated: Option<Truncated>,
}

/// How a command's call came to an end.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "ended", rename_all = "snake_case")]
pub enum Ended {
    Exited,
    TimedOut,
    /// Someone stopped it, the person or the agent.
    Stopped,
    /// It carries on in the background, as the task the hub lists.
    Backgrounded {
        task: TaskId,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Truncated {
    /// The file the whole of it is in, which the agent can read, when it kept one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_at: Option<String>,
}

/// Lines of a file, each with its number in the file, without what any agent puts in front of each.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Lines {
    /// The number of the first, from 1.
    pub first: u32,
    pub lines: Vec<String>,
    /// How many the file has, when the agent says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
}

/// What a call did to one file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FileOutcome {
    pub path: String,
    /// `None` where the agent doesn't say, as [`super::Capability::FileChanges`] says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<FileChange>,
    /// Where it went, when it moved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved_to: Option<String>,
    /// What its language server found wrong in it afterwards, as [`super::Capability::Diagnostics`] says.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// From 1.
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "found", rename_all = "snake_case")]
pub enum Found {
    Lines {
        lines: Vec<FoundLine>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        truncated: bool,
    },
    Files {
        paths: Vec<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        truncated: bool,
    },
    Count {
        matches: u64,
        files: u64,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FoundLine {
    pub path: String,
    /// From 1.
    pub line: u32,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WebResult {
    pub title: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpContent {
    Text {
        text: String,
    },
    Image {
        image: Attachment,
    },
    /// A resource the server pointed to or sent, by its URI, with its text when it sent some.
    Resource {
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    #[serde(other)]
    Unknown,
}
