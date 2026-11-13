//! The requests about the logins a hub holds for agents' accounts, which it hands to the workers it launches agents on.
//! A hub older than one refuses it, as `is_unsupported` says.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{Account, AccountId, AgentKind, LoginToken};

/// Gives the hub a login for agents of `agent`'s kind to run under, which it keeps and hands to the workers it launches
/// them on, and answers with the account's record, named `name`. The token is one the agent's own CLI made to last,
/// such as `claude setup-token`'s, which the person made themselves and gave a client, and the hub never sends it back.
///
/// With `account`, it replaces that account's login, as when its token is about to end, so the account keeps its
/// limits and its sessions. Without, a token the hub already holds for that kind finds its account. Either way,
/// sending it again does nothing more. Refused with `InvalidRequest` when the token is empty, and with `NotFound` when
/// there's no such account.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AccountConnect {
    pub agent: AgentKind,
    /// What to call the account, since a token doesn't say whose it is.
    pub name: String,
    pub token: LoginToken,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountId>,
}

impl Request for AccountConnect {
    type Reply = Account;

    fn into_request(self) -> ClientRequest {
        ClientRequest::AccountConnect(self)
    }
}

/// What a request about an account's login is answered with: the account's record, as it is then.
impl Reply for Account {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Account(account) = response else { return Err(response) };
        Ok(account)
    }
}

/// Has the hub forget the login it holds for the account, so no agent launched after this runs under it, and answers
/// with the account's record, which stays, as the sessions that ran under it name it. An agent already running under
/// it keeps it until it exits. It doesn't revoke the token, which works wherever else it's kept until it ends.
/// Disconnecting an account whose login the hub doesn't hold changes nothing, so sending it again does nothing more.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AccountDisconnect {
    pub id: AccountId,
}

impl Request for AccountDisconnect {
    type Reply = Account;

    fn into_request(self) -> ClientRequest {
        ClientRequest::AccountDisconnect(self)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The same as if its fields were the request's own.
    #[test]
    fn connecting_a_login_names_no_account_unless_it_replaces_one_and_hides_its_token_from_debug() {
        let wire = json!({"method": "account_connect", "agent": "claude", "name": "Work", "token": "a-made-up-token"});
        let request = ClientRequest::AccountConnect(AccountConnect {
            agent: AgentKind::Claude,
            name: "Work".into(),
            token: LoginToken::new("a-made-up-token"),
            account: None,
        });

        assert_eq!(serde_json::to_value(&request).unwrap(), wire);
        assert_eq!(serde_json::from_value::<ClientRequest>(wire).unwrap(), request);
        assert!(!format!("{request:?}").contains("made-up"));
    }
}
