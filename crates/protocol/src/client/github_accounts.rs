//! The requests about the GitHub accounts a hub holds, the default of which agents push and open pull requests as on
//! every machine. A hub older than these refuses them, as `is_unsupported` says, and its settings say nothing of
//! GitHub.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{GithubAccount, GithubAccountId, GithubToken, HubSettings};

/// Gives the hub a token the person made on GitHub, such as a fine-grained one, to hold as a GitHub account's login,
/// and answers with the account's record. The hub asks GitHub whose it is and when it ends, keeps it in a file of its
/// own, and never sends it back. The token is trimmed.
///
/// Without `id`, it makes a new account, which becomes the default if it's the first the hub has a record of. With
/// `id`, it renews that account's login in place, as when its token is about to end, so the account stays the default
/// if it was, and one whose login was disconnected has one again. A token the hub holds already is that account again,
/// and changes nothing, so sending it again does nothing more, and the hub doesn't ask GitHub. A token the hub held and
/// was told to forget makes a new account without `id`, since the hub keeps nothing to know it again by.
///
/// Refused with `InvalidRequest` when the token is empty, very long, or has a space or a control character inside, and
/// when GitHub doesn't take it as a user's, as with one that has ended or been revoked. Refused with `NotFound` when
/// the hub has no record of the account `id`, with `Conflict` when another account holds the token, or with `id` when
/// it's another GitHub user's than that account's, and with `Unavailable` when GitHub can't be reached, or doesn't
/// answer in time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GithubAccountConnect {
    pub token: GithubToken,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<GithubAccountId>,
}

impl Request for GithubAccountConnect {
    type Reply = GithubAccount;

    fn into_request(self) -> ClientRequest {
        ClientRequest::GithubAccountConnect(self)
    }
}

/// What a request about a GitHub account is answered with: its record, as it is then.
impl Reply for GithubAccount {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::GithubAccount(account) = response else { return Err(response) };
        Ok(account)
    }
}

/// Has the hub forget the login it holds for the account, and answers with the account's record, which stays, with no
/// login, since the default can name it. Disconnecting the default leaves it the default, holding no login that can be
/// used, until it's connected again or another is chosen. It doesn't revoke the token on GitHub, which works wherever
/// else it's kept until it ends. Disconnecting an account whose login the hub doesn't hold changes nothing, so sending
/// it again does nothing more. Refused with `NotFound` when the hub has no record of the account.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GithubAccountDisconnect {
    pub id: GithubAccountId,
}

impl Request for GithubAccountDisconnect {
    type Reply = GithubAccount;

    fn into_request(self) -> ClientRequest {
        ClientRequest::GithubAccountDisconnect(self)
    }
}

/// Chooses the default GitHub account, as [`GithubSettings::account`](crate::model::GithubSettings::account) says, and
/// answers with the hub's settings. `None` makes each machine's own login the default. Choosing the default again
/// changes nothing. Refused with `NotFound` when the hub has no record of the account, and with `Conflict` when its
/// login can't be used now, as when it's been disconnected or has ended, leaving the default as it was.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GithubAccountSetDefault {
    #[serde(default)]
    pub account: Option<GithubAccountId>,
}

impl Request for GithubAccountSetDefault {
    type Reply = HubSettings;

    fn into_request(self) -> ClientRequest {
        ClientRequest::GithubAccountSetDefault(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connecting_a_token_hides_it_from_debug() {
        let request = GithubAccountConnect { token: GithubToken::new("github_pat_made_up"), id: None }.into_request();

        assert!(!format!("{request:?}").contains("made_up"), "{request:?}");
    }
}
