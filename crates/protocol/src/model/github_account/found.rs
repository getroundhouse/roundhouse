use serde::{Deserialize, Serialize};

use super::super::{GithubUsername, ProviderName, Rev};

/// Who a provider's machines' own `gh` is logged in to github.com as, and whether GitHub took its token, as a record of
/// its own, keyed by the provider, so every client can show it beside the GitHub accounts the hub holds. Agents there use it only while the default is
/// the machine's own, as [`GithubSettings::account`](crate::model::GithubSettings::account) says. The local provider's
/// is this machine's. The hub asks the first worker of each provider it links to after it starts, as
/// [`FindGithubLogin`](crate::worker::FindGithubLogin) says, and forgets every one as it starts, so a provider with none
/// hasn't been asked since, or no worker there has been able to say. It never carries the token.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FoundGithubLogin {
    pub provider: ProviderName,
    pub login: GhLogin,
    #[serde(default)]
    pub rev: Rev,
}

/// What a machine's own `gh` is logged in to github.com with, as its worker found it, without the token. It's `gh`'s
/// alone: git can push through a credential helper of its own, or with an SSH key, which this doesn't find.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GhLogin {
    /// Logged in as `username`: as `gh`'s own config names it, or as GitHub said, for a token `gh` takes from where it
    /// runs, such as `GH_TOKEN`, which it uses over its own login.
    LoggedIn {
        username: GithubUsername,
        /// Whether GitHub took its token when the provider was asked, which is once a hub start, so not since.
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

/// Whether GitHub took a found login's token, by the HTTP status it answered the hub with when the hub asked whose the
/// token is, with the token its worker handed it, as [`FindGithubLogin`](crate::worker::FindGithubLogin) says. A
/// worker from before the hub asked said it from what GitHub answered `gh`. *Unconfirmed:* how it's said, and a success
/// naming another user being one that can't be told.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GhLoginWorks {
    /// GitHub answered with a success, naming the same user.
    Yes,
    /// GitHub refused the token, with 401, so `gh auth login` would log it in again.
    Refused,
    /// GitHub didn't answer, as when it can't be reached, or answered anything else, such as 403, a 5xx or another
    /// user, refused a token `gh` renews by itself, whose time may only be up, or the worker had no token to give.
    CouldntTell,
    /// From a newer peer, or one that didn't say.
    #[default]
    #[serde(other)]
    Unknown,
}
