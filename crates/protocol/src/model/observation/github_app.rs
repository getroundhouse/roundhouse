//! What the hub observes of an app of your own, which GitHub owns.

use serde::{Deserialize, Serialize};

use crate::model::{GithubAppSlug, GithubUser};

/// What GitHub says of an app of your own, as `GET /app` answers the app asking with its key, and as GitHub said as it
/// made the app. The app's number, its key and what it was made from are Roundhouse's, and stay on its record.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubAppSaid {
    /// What GitHub calls it in addresses, which it's installed from, at `https://github.com/apps/{slug}`, and which its
    /// bot is named for.
    pub slug: GithubAppSlug,
    /// What GitHub shows it as, as the person named it.
    pub name: String,
    /// The user or organization it's under, which is the one it can be installed on.
    pub owner: GithubUser,
}
