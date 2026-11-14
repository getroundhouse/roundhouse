//! The requests about the logins a hub holds for agents' accounts, which it hands to the workers it launches agents on
//! as the default for their kind. A hub older than one refuses it, as `is_unsupported` says.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{Account, AccountId, AgentKind, HubSettings, LoginToken};

/// Gives the hub a login for agents of `agent`'s kind to run under, which it keeps and hands to the workers it launches
/// them on, when selected as their kind's default, and answers with the account's record, named `name`.
/// The token is one the agent's own CLI made to last, such as `claude setup-token`'s, which the person made themselves
/// and gave a client, and the hub never sends it back.
///
/// With `account`, it renews that account's login, replacing it, as when its token is about to end, so the account
/// keeps its limits and its sessions, and one whose login was disconnected has one again. With the token that account
/// holds already, only its name changes, since its login is the same. Without, it makes a new account, but for a token
/// the hub already holds for that kind: under that account's name it finds the account and changes nothing, as when a
/// client sends the same again, and under another it's refused, since the person meant to add an account. A token the
/// hub held and was told to forget makes a new account, since the hub keeps nothing to know it again by. The token and
/// the name are trimmed.
///
/// Refused with `InvalidRequest` when the name is empty or longer than 200 bytes, when the token is empty, very long,
/// or has a space or a control character inside, when the hub can hold no login for that kind, as its
/// [`AgentDescription::login`](crate::model::AgentDescription::login) says, and when `account` names one whose ID the
/// hub didn't choose for a login of that kind, since nothing says the token is that account's. Refused with `NotFound`
/// when there's no such account, and with `Conflict` when another account holds the same token, or, without
/// `account`, when the one that holds it has another name.
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
/// it keeps it until it exits. It doesn't revoke the token, which works wherever else it's kept until it ends. Refused
/// with `NotFound` when there's no such account.
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

/// Chooses the hub-wide default for new processes of this kind. The account must hold an unexpired
/// login of the same kind. A disconnected or expired default remains selected and blocks new starts.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AccountSetDefault {
    pub agent: AgentKind,
    pub account: AccountId,
}

impl Request for AccountSetDefault {
    type Reply = HubSettings;

    fn into_request(self) -> ClientRequest {
        ClientRequest::AccountSetDefault(self)
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
