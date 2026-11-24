use serde::{Deserialize, Serialize};

use super::super::GithubUsername;

/// Who `gh` and the SSH key on the hub's own machine are on GitHub, as
/// [`HubMachineLogins`](crate::client::github_accounts::HubMachineLogins) answers, so Settings' Providers shows them
/// beside who the hub reads GitHub as, and a person sees when they differ. Agents on the local provider are whoever
/// these are, since Roundhouse hands them nothing for GitHub there.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MachineLogins {
    /// Who `gh` is, as [`HubGhLogin`](crate::client::github_accounts::HubGhLogin) says, or `None` when the hub couldn't
    /// tell, as when `gh` didn't answer in time, or printed what the hub can't read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gh: Option<GhLogin>,
    pub ssh: SshLogin,
}

/// Whose SSH key a machine pushes to github.com with, as GitHub names it to `ssh -T git@github.com`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SshLogin {
    /// GitHub took the key as `username`'s.
    Key { username: GithubUsername },
    /// GitHub took no key from the machine.
    NoKey,
    /// GitHub couldn't be asked, or didn't say.
    CouldntTell,
    /// From a newer peer.
    #[serde(other)]
    Unknown,
}

/// What the hub's machine's own `gh` is logged in to github.com with, as the hub found it, without the token. It's `gh`'s
/// alone: git can push through a credential helper of its own, or with an SSH key, which this doesn't find.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GhLogin {
    /// Logged in as `username`: as `gh`'s own config names it, or as GitHub said, for a token `gh` takes from where it
    /// runs, such as `GH_TOKEN`, which it uses over its own login.
    LoggedIn {
        username: GithubUsername,
        /// Whether GitHub took its token when the hub asked, as it reads `gh`.
        #[serde(default)]
        works: GhLoginWorks,
    },
    /// `gh` is there, and logged in to nothing for github.com, so `gh auth login` would log it in.
    LoggedOut,
    /// There's no `gh` there, so it would have to be installed first.
    NotInstalled,
    /// One from a newer peer.
    #[serde(other)]
    Unknown,
}

/// Whether GitHub took the token of `gh`'s login, by the HTTP status it answered the hub with when the hub asked whose
/// the token is. *Unconfirmed:* how it's said, and a success naming another user being one that can't be told.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GhLoginWorks {
    /// GitHub answered with a success, naming the same user.
    Yes,
    /// GitHub refused the token, with 401, so `gh auth login` would log it in again.
    Refused,
    /// GitHub didn't answer, as when it can't be reached, or answered anything else, such as 403, a 5xx or another
    /// user, refused a token `gh` renews by itself, whose time may only be up, or `gh` printed no token.
    CouldntTell,
    /// From a newer peer, or one that didn't say.
    #[default]
    #[serde(other)]
    Unknown,
}
