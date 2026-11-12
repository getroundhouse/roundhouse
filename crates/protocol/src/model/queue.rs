//! A session's queue: prompts waiting for the agent to finish, each sent as a turn of its own.

use serde::{Deserialize, Serialize};

use super::ClientMessageId;
use crate::attachment::Attachment;
use crate::prompt::Prompt;

/// A prompt waiting its turn. The hub sends the first in a session's queue once the agent has nothing left to do, as a
/// turn of its own, unless the queue is held. It's held once the person stops the agent, or the agent goes away
/// mid-turn, and a prompt the person sends, queued or not, lets it go on once that prompt's turn ends. Sending it
/// records it as a prompt under the same `client_msg_id`, so it keeps one identity from queued to recorded.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct QueuedPrompt {
    pub client_msg_id: ClientMessageId,
    /// Its files were each added to the session before it was queued. They're the ones it was queued with, since
    /// changing a queued prompt changes only its text.
    #[serde(flatten)]
    pub prompt: Prompt<Attachment>,
}
