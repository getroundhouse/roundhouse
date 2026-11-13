//! What a hub asks of an agent's CLI outside any session, each in a call of its own: a title for a prompt, and the
//! account the CLI is logged in to. Neither changes anything, so sending one again only asks again.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::attachment::NewAttachment;
use crate::model::{AgentAccount, AgentKind};

/// A short title for a prompt, from a call of its own to a small model of `agent`'s.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TitleFor {
    pub agent: AgentKind,
    pub prompt: String,
    /// The prompt's images, read with its words, since a prompt such as "how do we fix this?" can be about a
    /// screenshot. Any that isn't an image, such as a kind from a newer hub, is left out. A worker from before images
    /// ignores them, and titles from the words alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<NewAttachment>,
}

impl Request for TitleFor {
    type Reply = TitleReply;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::TitleFor(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TitleReply {
    /// As the model answered, which can be anything.
    pub title: String,
}

impl Reply for TitleReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::Title(title) = response else { return Err(response) };
        Ok(title)
    }
}

/// The account `agent`'s CLI is logged in to on this machine. Every agent of a kind here runs under one login today.
/// When they can run under several, this says which one's it asks for.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AccountFor {
    pub agent: AgentKind,
}

impl Request for AccountFor {
    type Reply = AgentAccount;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::AccountFor(self)
    }
}

impl Reply for AgentAccount {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::Account(account) = response else { return Err(response) };
        Ok(account)
    }
}
