//! The agent's personas and styles, by the names it lists them under.

use serde::{Deserialize, Serialize};

use crate::model::ids;

ids!(
    /// One of the agent's personas, by the name it lists it under.
    PersonaName,
    /// One of the agent's styles, by the name it lists it under.
    StyleName,
);

/// A persona, as [`Capability::Personas`](super::Capability::Personas) lists them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Persona {
    pub name: PersonaName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub used_by: PersonaFor,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PersonaFor {
    /// A chat can run as it.
    Chats,
    /// Only the agent can hand work to it, as a subagent's persona.
    Subagents,
    Both,
    #[serde(other)]
    Unknown,
}
