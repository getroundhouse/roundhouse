use serde::{Deserialize, Serialize};

use super::super::Rev;
use super::{GithubAppId, GithubInstallationId, GithubUserNumber};

/// An app of your own: a GitHub App the person made under their own GitHub account, or an organization's, through
/// GitHub's manifest flow, whose private key the hub holds, as a record of its own, so every client shows the same of
/// it, such as that it's made and not yet installed. Agents act as its bot, such as `ada-roundhouse[bot]`, through its
/// installations, which are each a [`GithubAccount`](super::GithubAccount) of their own, as [`GithubInstallation`]
/// says. It's private, as it's made, so only its owner can install it, and it has at most one installation, on its
/// owner: the person makes one for each account or organization it's for. An app made public on GitHub later could
/// have more, which the record takes as it is, though the hub connects only its owner's. The key is in a file of the
/// hub's, and never on a record. What GitHub says of it, and its bot, are the observations `GithubAppSaid` and
/// `GithubAppBot`. *Unconfirmed:* an app as a record of its own, apart from its installations' accounts.
///
/// Removing it has the hub forget the key and every installation's token, and removes its record, as
/// `HubNotice::Removed` says. The app stays on GitHub, installed where it was, until the person deletes it there.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubApp {
    pub id: GithubAppId,
    pub rev: Rev,
}

/// Where an app of your own is installed, as a GitHub account's record says of one that's an installation of it. Each
/// installation is an account of its own, since each reaches the repositories of the user or organization it's on, as
/// far as the person chose.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubInstallation {
    pub app: GithubAppId,
    pub id: GithubInstallationId,
    /// The user or organization it's installed on, by GitHub's number for it. What GitHub calls it is the observation
    /// `InstalledOn`.
    pub on: GithubUserNumber,
}
