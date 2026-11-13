//! The requests about the logins a hub holds for agents' accounts, which it hands to the workers it launches agents on.
//! Each is the body of the `ClientRequest` of its name, which has the same form on the wire as if its fields were the
//! request's own. A hub older than one refuses it, as `is_unsupported` says.

use serde::{Deserialize, Serialize};

use crate::model::{AccountId, AgentKind, LoginToken};

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

/// Has the hub forget the login it holds for the account, so no agent launched after this runs under it, and answers
/// with the account's record, which stays, as the sessions that ran under it name it. An agent already running under
/// it keeps it until it exits. It doesn't revoke the token, which works wherever else it's kept until it ends.
/// Disconnecting an account whose login the hub doesn't hold changes nothing, so sending it again does nothing more.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AccountDisconnect {
    pub id: AccountId,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::client::ClientRequest;

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

    #[test]
    fn disconnecting_names_the_account_beside_its_method() {
        let request = ClientRequest::AccountDisconnect(AccountDisconnect { id: "claude-1".into() });
        let wire = json!({"method": "account_disconnect", "id": "claude-1"});

        assert_eq!(serde_json::to_value(&request).unwrap(), wire);
        assert_eq!(serde_json::from_value::<ClientRequest>(wire).unwrap(), request);
    }
}
