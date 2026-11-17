//! The requests about apps of your own: GitHub Apps the person makes under their own GitHub account, or an
//! organization's, through GitHub's manifest flow, whose key the hub holds, and whose installation is a GitHub account
//! agents can push and open pull requests as, as its bot. A hub older than these refuses them, as `is_unsupported`
//! says.
//!
//! Making one goes: a client asks the hub for the app's manifest, and sends it to GitHub's form under the account or
//! organization the app is for, `https://github.com/settings/apps/new` for the person's own, or
//! `https://github.com/organizations/{org}/settings/apps/new` for an organization's, since the app is private, so only
//! that one can install it. It listens on its own address for GitHub to send the person back with a code once they've
//! made the app, then gives the hub the code, and sends the person to install it. GitHub's answer to the code says
//! whose the app is, so the manifest doesn't.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{GithubApp, GithubAppId};

/// Asks the hub for the manifest of an app of your own, for the client to send GitHub's manifest flow, so it's the
/// hub that decides what an app asks for. `name` is what GitHub is to call it, which the person can change on GitHub
/// before they make it, and which GitHub refuses if another app has it. `redirect_url` is where GitHub sends the person
/// once they've made it, with the code to give [`GithubAppMake`], such as the client's own on `127.0.0.1`. The hub
/// keeps nothing of it, so asking again does nothing more.
///
/// Refused with `InvalidRequest` when `name` is empty or longer than GitHub takes, or `redirect_url` isn't an `http` or
/// `https` address.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GithubAppManifestWrite {
    pub name: String,
    pub redirect_url: String,
}

impl Request for GithubAppManifestWrite {
    type Reply = GithubAppManifest;

    fn into_request(self) -> ClientRequest {
        ClientRequest::GithubAppManifestWrite(self)
    }
}

/// An app of your own's manifest, as GitHub's manifest flow takes it: a JSON object, which the client sends GitHub as
/// it is, as the form's `manifest`, so a client sends what a newer hub asks for too.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GithubAppManifest {
    pub manifest: serde_json::Value,
}

impl Reply for GithubAppManifest {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::GithubAppManifest(manifest) = response else { return Err(response) };
        Ok(manifest)
    }
}

crate::tokens!(
    /// The code GitHub sends the person back with once they've made an app from its manifest, which the hub gives
    /// GitHub, within an hour, for the app's key. Until then, it's all it takes to have the key, so it's a secret.
    GithubManifestCode,
);

/// Gives the hub the code GitHub sent the person back with once they made an app of your own, for the hub to have
/// GitHub make it into the app's key and keep it, and answers with the app's record. Then the hub asks GitHub for the
/// app's bot and where it's installed, and makes a GitHub account of each installation, as
/// [`GithubAppInstallationsConnect`] does. Any of those can fail, as before the person has installed the app, which
/// leaves the app made, for a client to connect its installations later. A hub that has made an app from this code
/// already answers with its record, without asking GitHub, so sending it again does nothing more.
///
/// Refused with `InvalidRequest` when GitHub doesn't know the code, as once it has been used or has ended, and with
/// `Unavailable` when GitHub can't be reached, doesn't answer in time, or answers with what isn't an app, each of which
/// leaves every record as it was.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GithubAppMake {
    pub code: GithubManifestCode,
}

impl Request for GithubAppMake {
    type Reply = GithubApp;

    fn into_request(self) -> ClientRequest {
        ClientRequest::GithubAppMake(self)
    }
}

/// What a request about an app of your own is answered with: its record, as it is then, or as it last was once it's
/// been removed.
impl Reply for GithubApp {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::GithubApp(app) = response else { return Err(response) };
        Ok(app)
    }
}

/// Has the hub look for where the app is installed now, as when the person has just installed it, and connect its
/// installation on the app's owner, which is the only one a private app has: making a GitHub account for it when the
/// hub has none, connecting it again when it was disconnected, or when it was installed again where it was removed.
/// It's an account of its own, whose user is the app's bot, and the first account the hub has a record of becomes the
/// default. One elsewhere, as once the person has made the app public on GitHub, isn't connected. Then answers with
/// the app's record. Every client is told of each account it made or changed before the answer.
///
/// The hub also looks by itself as it checks its GitHub logins, once a minute, but connects none that was
/// disconnected, since that was the person's choice. Either way, an installation whose account holds a login and that
/// isn't there any more, as when it was uninstalled on GitHub, has its login forgotten, as disconnecting it would.
///
/// Refused with `NotFound` when the hub has no record of the app, with `Conflict` when it can't use the app's key, and
/// with `Unavailable` when GitHub can't be reached, doesn't answer in time, or refuses the key, as when the app has
/// been deleted on GitHub. What was connected before a failure stays connected.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GithubAppInstallationsConnect {
    pub id: GithubAppId,
}

impl Request for GithubAppInstallationsConnect {
    type Reply = GithubApp;

    fn into_request(self) -> ClientRequest {
        ClientRequest::GithubAppInstallationsConnect(self)
    }
}

/// Has the hub forget an app of your own: the token of each of its installations, as disconnecting each would, and
/// then its key, and answers with its record as it last was, which every client is then told has been removed. Each
/// installation's account stays, holding no login, since the default can name one, so a default that was one blocks
/// until another is chosen. It doesn't delete the app on GitHub, or uninstall it, which the person does there. Refused
/// with `NotFound` when the hub has no record of the app, as once it has been removed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GithubAppRemove {
    pub id: GithubAppId,
}

impl Request for GithubAppRemove {
    type Reply = GithubApp;

    fn into_request(self) -> ClientRequest {
        ClientRequest::GithubAppRemove(self)
    }
}
