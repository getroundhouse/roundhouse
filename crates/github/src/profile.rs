use serde::{Deserialize, Serialize};

/// What GitHub's profile of a user says that its commit identity is made from, as `GET /user` gives it: the name the
/// user goes by and the email it shows everyone, either of which GitHub gives as none, as for a user who hasn't set a
/// name or keeps every email private. The hub keeps it in its cache file in its serde form.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct GithubProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}
