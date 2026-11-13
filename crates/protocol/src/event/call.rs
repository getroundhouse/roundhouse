//! What kind of call a tool's use is. The agent's adapter says, since only it knows the agent's tools, so a client
//! draws a command or an edit from this rather than from any agent's name for the tool. What the call was given stays
//! the agent's own input, which a client reads knowing what kind of call it's reading.

use serde::{Deserialize, Serialize};

/// A kind of call. One this build doesn't know reads as `Unknown`, which a client draws as it would a tool with no
/// kind of its own, by the tool's name.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CallKind {
    /// Running a shell command.
    Command,
    /// Reading a file.
    Read,
    /// Writing a whole file.
    Write,
    /// Changing part of a file.
    Edit,
    /// Looking through what files hold for a pattern.
    Search,
    /// Looking for files by their names.
    FindFiles,
    /// Reading a page from the web.
    Fetch,
    /// Searching the web.
    WebSearch,
    /// Handing work to a subagent.
    Subagent,
    /// Asking the advisor, a stronger model that reads the whole conversation and advises the agent.
    Advisor,
    /// Asking the person questions, which its request for approval carries, as [`super::Asking::Questions`]. It's a
    /// kind of call too, so a client can tell from the call alone that its approval is what draws it, without the
    /// approval being loaded, and without knowing the agent's name for the tool.
    Questions,
    /// Asking the person to approve its plan, which its request for approval carries, as [`super::Asking::Plan`], and
    /// a kind of call for the same reason.
    Plan,
    /// A tool with no kind of its own, such as one from an MCP server.
    Other {
        /// The MCP server it's from, if it's from one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        server: Option<String>,
        /// The tool's name, as the agent or its server has it, without the server's.
        #[serde(default)]
        tool: String,
    },
    /// One this build doesn't know. An adapter also says it of a result whose call it no longer knows, such as one
    /// the agent answers after it was started again, so that every result it records says a kind.
    #[serde(other)]
    Unknown,
}
