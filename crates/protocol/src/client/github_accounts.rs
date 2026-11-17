//! The requests about the GitHub accounts a hub holds, the default of which agents push and open pull requests as on
//! every machine, and about signing in with GitHub to connect one. A hub older than these refuses them, as
//! `is_unsupported` says, and its settings say nothing of GitHub. One that holds GitHub accounts but can't sign in with
//! GitHub refuses only the sign-in's.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{GithubAccount, GithubAccountId, GithubSignIn, GithubSignInId, GithubToken, HubSettings};

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
/// else it's kept until it ends. Disconnecting an app of your own's installation has the hub stop making its tokens,
/// and the hub's own looks for where the app is installed leave it disconnected while the app stays installed there:
/// only [`GithubAppInstallationsConnect`](super::github_apps::GithubAppInstallationsConnect) connects it again. It
/// doesn't uninstall the app on GitHub. Disconnecting an account whose login the hub doesn't hold changes nothing, so
/// sending it again does nothing more. Refused with `NotFound` when the hub has no record of the account.
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

/// Starts signing in with GitHub, through the hub's OAuth App and GitHub's device flow, and answers with the sign-in's
/// record, waiting with the code for the person to type and where to type it. From then on the hub asks GitHub how it
/// stands, whether or not any client is there, and every client is told of its record as it changes. Approved, it
/// makes a GitHub account, which becomes the default if it's the first the hub has a record of, or renews the login of
/// the account the hub holds a sign-in for as the same GitHub user. A GitHub account held by a token is another, even
/// as the same user. Starting one removes the record of every sign-in that has ended.
///
/// `id` is the client's own, so sending it again is the same sign-in, and answers with its record as it is then,
/// however it has gone, without asking GitHub again. Once its record has gone, the same `id` starts a new one.
///
/// Refused with `Conflict` when as many sign-ins as may wait at once are waiting, and with `Unavailable` while the hub
/// stops, or when GitHub can't be reached, doesn't answer in time, or won't give a code, as when it's limiting how
/// often it's asked, each of which leaves every record as it was.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GithubSignInStart {
    pub id: GithubSignInId,
}

impl Request for GithubSignInStart {
    type Reply = GithubSignIn;

    fn into_request(self) -> ClientRequest {
        ClientRequest::GithubSignInStart(self)
    }
}

/// What a request about a sign-in with GitHub is answered with: its record, as it is then.
impl Reply for GithubSignIn {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::GithubSignIn(sign_in) = response else { return Err(response) };
        Ok(sign_in)
    }
}

/// Cancels a sign-in that's waiting, so the hub stops asking GitHub, and answers with its record. A sign-in GitHub has
/// approved meanwhile stays approved, with its account. Cancelling one that has ended changes nothing, so sending it
/// again does nothing more. Refused with `NotFound` when the hub has no record of it, as once it has gone.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GithubSignInCancel {
    pub id: GithubSignInId,
}

impl Request for GithubSignInCancel {
    type Reply = GithubSignIn;

    fn into_request(self) -> ClientRequest {
        ClientRequest::GithubSignInCancel(self)
    }
}
