//! What the agent is given as it starts, what it loaded, the tools it reaches, and calls outside a conversation.

use serde::{Deserialize, Serialize};

use super::Claimed;
use super::area::area;
use crate::model::ids;

area! { "the agent's setup";
    /// Roundhouse's instructions reach the agent's system prompt at every launch, a resumed one included, and each of
    /// its subagents' too.
    Instructions => instructions: Claimed,
    /// The worker's tools reach the agent over MCP, at a URL, with a token in a header of every request, for as long
    /// as the agent runs. The token is never where the agent's commands, or git, can read it: not in the worktree, not
    /// in a command's environment, and not echoed back in what the agent reports of its servers.
    ToolServer => tool_server: Claimed,
    /// The instruction files the agent loaded, such as a project's `AGENTS.md`, are reported as it starts, and again
    /// as it loads more, and the hub keeps the newest on the session's record.
    InstructionFiles => instruction_files: Claimed,
    /// Each tool server the agent has is reported with its state, such as connected, needing sign-in or failed, whole
    /// whenever one changes, and the hub keeps the newest on the session's record. One that failed can be connected
    /// again without starting the agent again.
    ToolServers => tool_servers: Claimed,
    /// A tool server that needs signing in to can be signed in to from a client, as all three agents do with MCP's
    /// sign-in: the page to open comes as the server's request, as [`super::asking`] has it, and the server's state
    /// says once it's done.
    ToolServerSignIn => tool_server_sign_in: Claimed,
    /// The agent's hooks are reported as they run, each with what it said and whether it stopped anything, which the
    /// hub records.
    Hooks => hooks: Claimed,
    /// A one-off call, with no conversation, answers a prompt with the reply alone, on a cheap model, with no tools,
    /// no project's instructions and no hooks, for titles and the like.
    OneOff => one_off: Claimed,
}

ids!(
    /// One of the agent's tool servers, by the name it has it under.
    ServerName,
);

/// One of the agent's tool servers, as [`Capability::ToolServers`] reports it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ToolServerState {
    pub name: ServerName,
    pub state: ServerState,
    /// Why, in the agent's words, for one that failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// How many tools it offers, once connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServerState {
    Starting,
    Connected,
    /// It needs the person to sign in to it.
    NeedsSignIn,
    Failed,
    Disabled,
    #[serde(other)]
    Unknown,
}

/// One of the agent's hooks having run, as [`Capability::Hooks`] reports it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HookRun {
    /// When it runs, in the agent's words, such as `PreToolUse`.
    pub event: String,
    pub outcome: HookOutcome,
    /// What it said, for the person or the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookOutcome {
    Passed,
    /// It stopped what it ran for, such as a tool's use or the turn's end.
    Blocked,
    Failed,
    #[serde(other)]
    Unknown,
}
