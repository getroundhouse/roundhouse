use std::fmt;

use serde::{Deserialize, Serialize};

use crate::capability::setup::ServerName;
use crate::worker::TOOLS_SERVER;

/// What a person calls a tool server, which is whatever they like, such as `Linear (Northwind)`, and what every client
/// shows. Agents call it [`ToolServerName::for_agents`]. This file is the one home of which text can be a name, what
/// agents make of one, and when two can't both be had.
///
/// Only [`ToolServerName::typed`] checks one. One that's read, from another process or the database, is as the hub that
/// kept it took it, by the rule of its own build, so a change to the rule leaves every record readable.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(transparent)]
pub struct ToolServerName(String);

/// Why what a person typed can't be a tool server's name.
#[derive(thiserror::Error, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolServerNameError {
    #[error("an MCP server needs a name")]
    Empty,
    #[error("an MCP server's name is at most {most} characters")]
    TooLong { most: usize },
    #[error("an MCP server's name can't have a line break or another control character in it")]
    ControlCharacter,
    #[error("agents have Roundhouse's own tools under that name, so the MCP server needs another")]
    RoundhousesOwn,
}

/// The longest name agents call a server by, which leaves room beside it for a tool's name, since a CLI makes each
/// tool's name from both. *Unconfirmed.*
const MOST_FOR_AGENTS: usize = 32;

/// What agents call a server whose name has nothing they can take, such as one in Japanese. *Unconfirmed.*
const NOTHING_TO_TAKE: &str = "server";

impl ToolServerName {
    /// Room for what a server is and whose account it's for. Past this it's more likely a paste by mistake.
    pub const MOST_CHARS: usize = 64;

    /// A name as a person typed it, trimmed, and otherwise as it was.
    pub fn typed(text: &str) -> Result<Self, ToolServerNameError> {
        let name = text.trim();
        if name.is_empty() {
            return Err(ToolServerNameError::Empty);
        }
        if name.chars().any(char::is_control) {
            return Err(ToolServerNameError::ControlCharacter);
        }
        if name.chars().count() > Self::MOST_CHARS {
            return Err(ToolServerNameError::TooLong { most: Self::MOST_CHARS });
        }
        let name = Self(name.to_owned());
        if name.for_agents().as_str().eq_ignore_ascii_case(TOOLS_SERVER) {
            return Err(ToolServerNameError::RoundhousesOwn);
        }
        Ok(name)
    }

    /// A name as the hub stored it. Only the hub's store calls this.
    pub fn stored(text: String) -> Self {
        Self(text)
    }

    /// What every agent calls the server, such as `Linear_Northwind` for `Linear (Northwind)`: its ASCII letters,
    /// digits and `-`, with their case, and one `_` for each run of anything else, with neither `_` nor `-` at an end.
    /// A CLI makes a tool's name from its server's, `__` between them, so a name with no `__` can't be the start of
    /// another server's tool's, and each CLI takes every character in it, as its reference in `docs/reference/` says.
    /// It's made each time it's asked for and stored nowhere, so a server that's renamed has its new one from each
    /// agent's next start.
    pub fn for_agents(&self) -> ServerName {
        let mut taken = String::with_capacity(self.0.len());
        for c in self.0.chars() {
            if c.is_ascii_alphanumeric() || c == '-' {
                taken.push(c);
            } else if !taken.ends_with('_') {
                taken.push('_');
            }
        }
        let at_an_end = ['_', '-'];
        let taken = taken.trim_matches(at_an_end);
        // All ASCII, so any byte is the end of a character.
        let taken = taken[..taken.len().min(MOST_FOR_AGENTS)].trim_end_matches(at_an_end);
        ServerName::from(if taken.is_empty() { NOTHING_TO_TAKE } else { taken })
    }

    /// Whether agents would call the two the same, in any case, so a hub can't have both. *Unconfirmed.*
    pub fn is_same_for_agents(&self, other: &ToolServerName) -> bool {
        self.for_agents().as_str().eq_ignore_ascii_case(other.for_agents().as_str())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ToolServerName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
