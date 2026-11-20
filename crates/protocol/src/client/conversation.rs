//! Conversation mode, in which the person talks with a chat's agent out loud, on one device at a time.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, Request};
use crate::model::{Session, SessionId};

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
