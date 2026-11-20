//! Conversation mode, in which the person talks with a chat's agent out loud, on one device at a time.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, Request};
use crate::model::{CutBy, Session, SessionId, SpokenThrough};

/// Turns conversation mode on in the chat `id`, with the conversation on the asking device, or moves the conversation
/// there from another device, which then stops hearing and speaking. Turning it on records the rule across the chat,
/// and moving it records nothing. Sending it again from the same device changes nothing. The hub refuses it from a
/// client that didn't say in its hello which device it is, and in a closed chat.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionConversationStart {
    pub id: SessionId,
}

impl Request for SessionConversationStart {
    type Reply = Session;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionConversationStart(self)
    }
}

/// Turns conversation mode off in the chat `id`, from any device. Sending it again changes nothing.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionConversationEnd {
    pub id: SessionId,
}

impl Request for SessionConversationEnd {
    type Reply = Session;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionConversationEnd(self)
    }
}

/// Says how far the asking device has read the agent's reply out loud in the chat `id`, about once a phrase, or with
/// `None` that it has read it to its end. Every client following the chat has it in the chat's tail, which only the
/// hub's memory keeps, so a hub that restarts has none until the next. Sending it again changes nothing. The hub refuses
/// it from a device that doesn't have the chat's conversation, which isn't speaking, and from a client that didn't say
/// in its hello which device it's on. The hub carries out a client's requests side by side, so a device sends each of
/// these, and a [`SessionVoiceCut`], only once the hub has answered the one before, or an older place could land last.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionSpeaking {
    pub id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub through: Option<SpokenThrough>,
}

impl Request for SessionSpeaking {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionSpeaking(self)
    }
}

/// Records that the voice reading the agent's reply out on the asking device in the chat `id` was cut off, `through`
/// that place in it, as [`crate::event::EventBody::VoiceCut`] says, and the chat's tail stops saying how far it has
/// got. A message is cut once, so sending it again, or another cut in the same message, records nothing more. Refused,
/// and sent in order, as [`SessionSpeaking`] is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionVoiceCut {
    pub id: SessionId,
    pub through: SpokenThrough,
    pub by: CutBy,
}

impl Request for SessionVoiceCut {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionVoiceCut(self)
    }
}
