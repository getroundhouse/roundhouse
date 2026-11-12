//! The requests that change a chat's queue of prompts, which the hub sends from in turn: one put at its end, changed,
//! taken out, or sent out of its turn. Each is the body of the `ClientRequest` of its name, which has the same form on
//! the wire as if its fields were the request's own.

use serde::{Deserialize, Serialize};

use crate::model::{AttachmentId, ClientMessageId, SessionId};

/// Puts a prompt at the end of the queue. Sending it again under the same `client_msg_id` changes nothing, so a
/// retry is safe, and `SessionChangeQueued` is what changes it. Refused with `Conflict` once that prompt has been
/// sent, since the agent has it by then. Answers with the session. On an idle agent it's sent at once, unless the
/// queue is held, as `Session::queue_held` says. As with `SessionPrompt`, each of `attachments` has to have been
/// added to this session first, and a prompt with any can have no text.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionQueue {
    pub id: SessionId,
    pub client_msg_id: ClientMessageId,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentId>,
}

/// Changes the text of the prompt queued under `client_msg_id`, which keeps its place and the files it was queued
/// with, so with any the text can be empty. Refused with `Conflict` once it's been sent, since the agent has it by
/// then, and with `NotFound` once it's been taken out, so a change that comes too late is never queued as a prompt
/// of its own. Answers with the session. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionChangeQueued {
    pub id: SessionId,
    pub client_msg_id: ClientMessageId,
    pub text: String,
}

/// Takes a prompt out of the queue. Taking out one that isn't there, because it's been sent or taken out already,
/// changes nothing. Answers with the session.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionUnqueue {
    pub id: SessionId,
    pub client_msg_id: ClientMessageId,
}

/// Sends the prompt queued under `client_msg_id` now, out of its turn: into the turn the agent is working on, which
/// steers it, or as a turn of its own. Like any prompt the person sends, it lets a held queue go on once its turn
/// ends. Sending one that isn't queued, because it's been sent or taken out, changes nothing. Answers with the
/// session once the prompt is recorded.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionSendQueued {
    pub id: SessionId,
    pub client_msg_id: ClientMessageId,
}
