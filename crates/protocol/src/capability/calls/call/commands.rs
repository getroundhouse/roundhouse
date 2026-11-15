//! Running a command, writing to one and watching one.

use serde::{Deserialize, Serialize};

use crate::model::ToolUseId;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub command: String,
    /// Where it runs, when that isn't the conversation's own directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// What the agent says it's for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub runs: Runs,
    /// What it does, as the agent's adapter reads the command: the files it reads, lists or searches. They never say
    /// it does less than it does: each part holds only where the program that does it does nothing else, and they're
    /// [`CommandPart::Other`] wherever the adapter can't tell. A client draws a command that does nothing but read one
    /// file as reading it. Empty where the adapter doesn't say.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<CommandPart>,
    /// It asks to run outside the sandbox, and why, in the agent's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsandboxed: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Runs {
    /// The call waits for it to end.
    #[default]
    Waits,
    /// It's left running as a background task, and the call ends at once.
    Background,
    /// In a terminal of its own, which the agent can write to and read from while it runs.
    Terminal,
    #[serde(other)]
    Unknown,
}

/// Something a command does, as its agent's adapter reads it, by a program that does nothing else. Its paths are as
/// the command names them, from where it ran.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "part", rename_all = "snake_case")]
pub enum CommandPart {
    /// Printing a file, or part of it, as `cat` and `sed -n` do.
    Read { path: String },
    /// Listing files, as `ls` does, in `path`, or where the command ran.
    List { path: Option<String> },
    /// Searching what's in files, as `rg` and `grep` do, for `pattern`, where the agent could say what for. A search
    /// of names, as `find` and `fd` make, is [`CommandPart::Other`], since they can do more with what they find.
    Search { pattern: Option<String>, path: Option<String> },
    /// Anything else, which says the command does more than the other parts say.
    #[serde(other)]
    Other,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WriteInput {
    /// The call that started the command.
    pub command: ToolUseId,
    /// What's written, which can be nothing, to read what it has printed since.
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Monitor {
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}
