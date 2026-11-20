//! How the person is talking with a chat's agent: typing, or out loud in conversation mode.

use serde::{Deserialize, Serialize};

use super::ClientDevice;

/// How the person is talking with a chat's agent, which every event is recorded under, so a prompt or a reply says
/// whether it was part of a conversation out loud. It's separate from the agent's own modes, such as plan mode and its
/// permission mode, which say what the agent may do rather than how the person talks with it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InteractionMode {
    /// The person types and reads, as in every chat until conversation mode is turned on.
    #[default]
    Typing,
    /// Conversation mode: the person talks with the agent out loud and hears it answer.
    Conversation,
    /// One this build doesn't know, from a newer hub.
    #[serde(other)]
    Unknown,
}

impl InteractionMode {
    pub fn is_typing(&self) -> bool {
        matches!(self, InteractionMode::Typing)
    }
}

/// How the person is talking with a chat's agent now, as its record says, with the device that has the conversation
/// while conversation mode is on.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Interaction {
    #[default]
    Typing,
    /// Conversation mode, with the conversation on `device`, the one that hears and speaks, until another takes it
    /// over. Boxed, since it's off in nearly every chat, and every copy of the record would otherwise carry room for
    /// two strings, which the feed's notices and the lists of records move by value.
    Conversation { device: Box<ClientDevice> },
    /// One this build doesn't know, from a newer hub.
    #[serde(other)]
    Unknown,
}

impl Interaction {
    pub fn mode(&self) -> InteractionMode {
        match self {
            Interaction::Typing => InteractionMode::Typing,
            Interaction::Conversation { .. } => InteractionMode::Conversation,
            Interaction::Unknown => InteractionMode::Unknown,
        }
    }

    pub fn is_typing(&self) -> bool {
        self.mode().is_typing()
    }

    /// The device that has the conversation, while conversation mode is on.
    pub fn device(&self) -> Option<&ClientDevice> {
        match self {
            Interaction::Conversation { device } => Some(device),
            Interaction::Typing | Interaction::Unknown => None,
        }
    }
}
