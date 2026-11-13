//! The requests about the GitHub accounts a hub holds, the default of which agents push and open pull requests as on
//! every machine. Which is the default is a setting of the hub's, changed with
//! [`HubConfigure`](super::admin::HubConfigure). A hub older than these refuses them, as `is_unsupported` says, and its
//! settings say nothing of GitHub.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{GithubAccount, GithubAccountId, GithubToken};

/// Gives the hub a token the person made on GitHub, such as a fine-grained one, to hold as an account's login, and
/// answers with the account's record. The hub asks GitHub whose it is and when it ends, keeps it in a file of its own,
/// and never sends it back. The token is trimmed.
///
/// With `id`, it renews that account's login in place, as when its token is about to end, so the account stays the
/// default if it was. Without, it makes a new account, and connecting while there's no default makes it the default.
/// The same token again finds the account that holds it and changes nothing, so sending it again does nothing more.
/// Another token without `id`, even for a user the hub holds already, is another account, so a token can stand beside
/// a sign-in that an organization won't take.
///
/// Refused with `InvalidRequest` when the token is empty, very long, or has a space or a control character inside,
/// when GitHub doesn't take it as a user's, as with one that has ended or been revoked, and when `id` names an account
/// whose login isn't a token, since signing in again renews a sign-in. Refused with `NotFound` when the hub holds no
/// account `id`, with `Conflict` when the token is another GitHub user's than that account's, or another account holds
/// it, and with `Unavailable` when GitHub can't be reached.
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

/// What a request about a GitHub account is answered with: its record, as it is then, or for one that removed it, as
/// it last was.
impl Reply for GithubAccount {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::GithubAccount(account) = response else { return Err(response) };
        Ok(account)
    }
}

/// Has the hub forget the account and its login, and answers with its record as it last was. The feed's removal of
/// the record is what says it's gone. It doesn't revoke the login, which works wherever else it's kept until it ends.
/// Disconnecting the default leaves none, rather than making another the default, so agents started under it can't
/// push to GitHub until they start again, under their machine's own login, as
/// [`GitCredentialSet`](crate::worker::GitCredentialSet) says. Refused with `NotFound` when the hub holds no such
/// account, as when it was disconnected already, which a client takes as the record being gone.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connecting_a_token_hides_it_from_debug() {
        let request = GithubAccountConnect { token: GithubToken::new("github_pat_made_up"), id: None }.into_request();

        assert!(!format!("{request:?}").contains("made_up"), "{request:?}");
    }
}
