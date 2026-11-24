//! What a hub asks of an agent's CLI outside any session, each in a call of its own: a title for a prompt, and the
//! account the CLI runs under. Neither changes anything, so sending one again only asks again. Each carries the login
//! the hub holds for the account, if it holds one, as an agent's launch does.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::attachment::NewAttachment;
use crate::model::{AgentAccount, AgentKind, AgentLogin};

/// A short title for a prompt, from a call of its own to a small model of `agent`'s.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TitleFor {
    pub agent: AgentKind,
    pub prompt: String,
    /// The prompt's images, read with its words, since a prompt such as "how do we fix this?" can be about a
    /// screenshot. Any that isn't an image, such as a kind from a newer hub, is left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<NewAttachment>,
    /// The login to make the call under, as [`AgentSpec::login`](super::AgentSpec::login) says of an agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<AgentLogin>,
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

/// The account `agent`'s CLI runs under here, with `login` or with this machine's own.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AccountFor {
    pub agent: AgentKind,
    /// The login an agent launched with it runs under, as [`AgentSpec::login`](super::AgentSpec::login) says. `None`
    /// reads this machine's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<AgentLogin>,
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
