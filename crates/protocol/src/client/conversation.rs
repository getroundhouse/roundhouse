//! Conversation mode, in which the person talks with a chat's agent out loud, on one device at a time.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, Request};
use crate::model::{Ending, MessageId, Session, SessionId};

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

/// Records how the agent's reply `message` in the chat `id` ended for the person hearing it, as the asking device read
/// it out, as [`crate::model::ReplyHeard`] says, which every device following the feed is then sent. The first the device
/// says of a reply is kept, so sending it again, or another of the same reply, changes nothing. The hub refuses it from
/// a device that doesn't have the chat's conversation, so a device says how a reply ended before it turns the mode off
/// or moves it, and from a client that didn't say in its hello which device it's on. A hub older than this refuses it
/// as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionReplyHeard {
    pub id: SessionId,
    pub message: MessageId,
    pub ending: Ending,
}

impl Request for SessionReplyHeard {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionReplyHeard(self)
    }
}
