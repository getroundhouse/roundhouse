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
    /// screenshot. Any that isn't an image, such as a kind from a newer hub, is left out. A worker from before images
    /// ignores them, and titles from the words alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<NewAttachment>,
    /// The login to make the call under, as [`AgentSpec::login`](super::AgentSpec::login) says of an agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<AgentLogin>,
}

impl Request for TitleFor {
    type Reply = TitleReply;

    fn into_request(self) -> WorkerRequest {
        if self.login.is_some() { WorkerRequest::TitleUnderLogin(self) } else { WorkerRequest::TitleFor(self) }
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_title_or_an_account_read_without_a_login_is_as_it_was_before_logins() {
        let titling = json!({"method": "title_for", "agent": "claude", "prompt": "fix the build"});
        let title = TitleFor { agent: AgentKind::Claude, prompt: "fix the build".into(), images: vec![], login: None };
        let asking = json!({"method": "account_for", "agent": "claude"});
        let account = AccountFor { agent: AgentKind::Claude, login: None };

        assert_eq!(serde_json::to_value(title.clone().into_request()).unwrap(), titling);
        assert_eq!(serde_json::from_value::<WorkerRequest>(titling).unwrap(), title.into_request());
        assert_eq!(serde_json::to_value(account.clone().into_request()).unwrap(), asking);
        assert_eq!(serde_json::from_value::<WorkerRequest>(asking).unwrap(), account.into_request());
    }
}
