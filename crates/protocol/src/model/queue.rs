//! A session's queue: prompts waiting for the agent to finish, each sent as a turn of its own.

use serde::{Deserialize, Serialize};

use super::ClientMessageId;
use crate::attachment::Attachment;
use crate::prompt::{Prompt, PromptFrom};

/// A prompt waiting its turn. The hub sends the first in a session's queue once the agent has nothing left to do, as a
/// turn of its own, unless the queue is held. It's held once the person stops the agent, the agent goes away mid-turn
/// or fails its turn, or the hub can't give the agent the prompt that would start a turn, since then it's for the
/// person to say whether to go on. A prompt the person sends, queued or not, lets it go on once that prompt's turn
/// ends. Sending it records it as a prompt under the same `client_msg_id`, so it keeps one identity from queued to
/// recorded.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct QueuedPrompt {
    pub client_msg_id: ClientMessageId,
    /// Its files were each added to the session before it was queued. They're the ones it was queued with, since
    /// changing a queued prompt changes only its text.
    #[serde(flatten)]
    pub prompt: Prompt<Attachment>,
    /// Who wrote it, which goes with it to the prompt it's recorded as: the person, or the hub, such as when it tells
    /// the agent the person's answer to a proposal.
    #[serde(default, skip_serializing_if = "PromptFrom::is_person")]
    pub from: PromptFrom,
}
