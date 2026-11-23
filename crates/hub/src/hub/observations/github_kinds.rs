//! The kinds of what GitHub says of what the hub holds of GitHub: who each GitHub account it holds a login for is.

use protocol::model::{GithubAccountId, Observation, ObservationKey, Observed, RecordKey, Rev};

use super::kept::Table;
use super::kinds::{Form, Kind};
use super::tables::Tables;

/// What GitHub calls a GitHub account's user now, as the hub last asked GitHub, sent at the hub's level: as the account
/// is connected, and again each day, or for an app's installation, whose user is the app's bot, as its app's record
/// says. A client of a version from before it reads it on the account's record.
pub(crate) struct GithubUsername;

impl Kind for GithubUsername {
    type Of = GithubAccountId;
    type Value = protocol::model::GithubUsername;
    const FORM: Form = Form { table: "github_username", version: 1 };

    fn observation(account: &GithubAccountId, username: Observed<Self::Value>, rev: Rev) -> Option<Observation> {
        Some(Observation::GithubUsername { account: account.clone(), username, rev })
    }

    fn key(account: &GithubAccountId) -> Option<ObservationKey> {
        Some(ObservationKey::GithubUsername { account: account.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.github_username
    }

    fn old_form_on(account: &GithubAccountId) -> Option<RecordKey> {
        Some(RecordKey::GithubAccount { id: account.clone() })
    }
}

/// What GitHub's profile of a GitHub account's user says, its name and public email, as the hub last asked GitHub with
/// the account's token, which its commit identity is made from. Kept by the hub, which sends it only to workers, in
/// what they give git, and to no client. An app's installation has none, since its bot has no profile.
pub(crate) struct GithubProfile;

impl Kind for GithubProfile {
    type Of = GithubAccountId;
    type Value = github::GithubProfile;
    const FORM: Form = Form { table: "github_profile", version: 1 };

    fn observation(_: &GithubAccountId, _: Observed<Self::Value>, _: Rev) -> Option<Observation> {
        None
    }

    fn key(_: &GithubAccountId) -> Option<ObservationKey> {
        None
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.github_profile
    }
}
