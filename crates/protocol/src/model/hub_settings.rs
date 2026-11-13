//! The settings a hub carries out itself, whichever client chose them, so it keeps them and every client shows the
//! same.

use serde::{Deserialize, Serialize};

use super::{GithubAccountId, Rev};

/// A hub's settings. There's one record of them per hub.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HubSettings {
    /// Whether it keeps its machine from going to sleep by itself while there's work.
    pub keep_awake: bool,
    /// Whether it allows a tool's use itself when an agent with full access asks anyway, as Claude's CLI does before
    /// a command it can't check, so nobody waits on it. `None` from a hub from before it could.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_asked_in_full_access: Option<bool>,
    /// Which of its GitHub accounts agents push as. `None` from a hub that doesn't keep GitHub accounts, which is how a
    /// client tells it from one that holds none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubSettings>,
    pub rev: Rev,
}

/// What the settings of a hub that keeps GitHub accounts say of them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubSettings {
    /// The default: the [`GithubAccount`](super::GithubAccount) agents push and open pull requests as, on every
    /// machine, for every repository on GitHub. `None` while there's none, and then each machine's own login is used,
    /// as [`FoundGithubLogin`](super::FoundGithubLogin) says. Connecting an account while there's none makes it the
    /// default, and disconnecting the default leaves none, so who agents push as changes only as the person chooses.
    #[serde(default)]
    pub account: Option<GithubAccountId>,
}

/// Settings to change. One left out is left as it is.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct HubSettingsChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_awake: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_asked_in_full_access: Option<bool>,
    /// Which GitHub account is the default, or that none is, as [`GithubSettings::account`] says. Refused with
    /// `NotFound`, changing nothing, when it names an account the hub doesn't hold. A hub that doesn't keep GitHub
    /// accounts leaves it, as it does a setting it doesn't know, and answers with settings that say nothing of GitHub.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_default: Option<GithubDefault>,
}

/// The GitHub account to make the default, or none, which a change says apart from leaving the default as it is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubDefault {
    /// `None` leaves no default.
    #[serde(default)]
    pub account: Option<GithubAccountId>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn settings_say_nothing_of_github_from_a_hub_that_keeps_no_accounts_and_say_so_from_one_that_holds_none() {
        let older = json!({"keep_awake": true, "rev": 3});
        let none = json!({"keep_awake": true, "github": {"account": null}, "rev": 3});
        let held = json!({"keep_awake": true, "github": {"account": "g"}, "rev": 3});

        let read = |settings| serde_json::from_value::<HubSettings>(settings).unwrap().github;

        assert_eq!(read(older), None);
        assert_eq!(read(none), Some(GithubSettings { account: None }));
        assert_eq!(read(held), Some(GithubSettings { account: Some("g".into()) }));
    }

    /// Left out leaves the default as it is, where an account of none says there's to be none.
    #[test]
    fn a_change_tells_leaving_the_default_from_making_none_the_default() {
        let read = |change| serde_json::from_value::<HubSettingsChange>(change).unwrap().github_default;

        assert_eq!(read(json!({"keep_awake": true})), None);
        assert_eq!(read(json!({"github_default": {"account": null}})), Some(GithubDefault { account: None }));
        assert_eq!(read(json!({"github_default": {}})), Some(GithubDefault { account: None }));
        assert_eq!(
            read(json!({"github_default": {"account": "g"}})),
            Some(GithubDefault { account: Some("g".into()) })
        );
    }
}
