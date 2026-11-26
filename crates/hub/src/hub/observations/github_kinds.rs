//! The kinds of what GitHub says of what the hub holds of GitHub: whether its default account can push to each
//! repository, who each GitHub account it holds a login for is, and what each app of your own is, and where each of
//! its installations is.

use protocol::model::{
    GithubAccountId, GithubAppId, GithubUser, Observation, ObservationKey, Observed, Reach, RepoId, Rev,
};

use super::keepers::Unkept;
use super::kept::Table;
use super::kinds::{Form, Kind};
use super::tables::Tables;

/// Whether the default GitHub account can push to a repository's repository on GitHub, as GitHub last answered the
/// hub's check, or as the hub says without asking, sent at the hub's level.
pub(crate) struct RepositoryReach;

impl Kind for RepositoryReach {
    type Of = RepoId;
    type Value = Reach;
    const FORM: Form = Form { table: "repository_reach", version: 1 };
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(repo: &RepoId, reach: Observed<Reach>, rev: Rev) -> Option<Observation> {
        Some(Observation::RepositoryReach { repo: repo.clone(), reach, rev })
    }

    fn key(repo: &RepoId) -> Option<ObservationKey> {
        Some(ObservationKey::RepositoryReach { repo: repo.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.repository_reach
    }
}

/// What GitHub calls a GitHub account's user now, as the hub last asked GitHub, sent at the hub's level: as the account
/// is connected, and again each day, or for an app's installation, whose user is the app's bot, as its app's record
/// says.
pub(crate) struct GithubUsername;

impl Kind for GithubUsername {
    type Of = GithubAccountId;
    type Value = protocol::model::GithubUsername;
    const FORM: Form = Form { table: "github_username", version: 1 };
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(account: &GithubAccountId, username: Observed<Self::Value>, rev: Rev) -> Option<Observation> {
        Some(Observation::GithubUsername { account: account.clone(), username, rev })
    }

    fn key(account: &GithubAccountId) -> Option<ObservationKey> {
        Some(ObservationKey::GithubUsername { account: account.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.github_username
    }
}

/// What GitHub says of an app of your own, its slug, name and owner, as the app last asked GitHub with its key, or as
/// GitHub said as it made it, sent at the hub's level.
pub(crate) struct GithubAppSaid;

impl Kind for GithubAppSaid {
    type Of = GithubAppId;
    type Value = protocol::model::GithubAppSaid;
    const FORM: Form = Form { table: "github_app_said", version: 1 };
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(app: &GithubAppId, said: Observed<Self::Value>, rev: Rev) -> Option<Observation> {
        Some(Observation::GithubAppSaid { app: *app, said, rev })
    }

    fn key(app: &GithubAppId) -> Option<ObservationKey> {
        Some(ObservationKey::GithubAppSaid { app: *app })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.github_app_said
    }
}

/// An app of your own's bot, as GitHub said it, asked as nobody by the app's slug, sent at the hub's level. Apart from
/// [`GithubAppSaid`], since GitHub says it in another call, as of another time.
pub(crate) struct GithubAppBot;

impl Kind for GithubAppBot {
    type Of = GithubAppId;
    type Value = GithubUser;
    const FORM: Form = Form { table: "github_app_bot", version: 1 };
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(app: &GithubAppId, bot: Observed<GithubUser>, rev: Rev) -> Option<Observation> {
        Some(Observation::GithubAppBot { app: *app, bot, rev })
    }

    fn key(app: &GithubAppId) -> Option<ObservationKey> {
        Some(ObservationKey::GithubAppBot { app: *app })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.github_app_bot
    }
}

/// What GitHub calls the user or organization an app's installation, a GitHub account, is on, as the hub last listed
/// the app's installations, sent at the hub's level.
pub(crate) struct InstalledOn;

impl Kind for InstalledOn {
    type Of = GithubAccountId;
    type Value = protocol::model::GithubUsername;
    const FORM: Form = Form { table: "installed_on", version: 1 };
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation(account: &GithubAccountId, on: Observed<Self::Value>, rev: Rev) -> Option<Observation> {
        Some(Observation::InstalledOn { account: account.clone(), on, rev })
    }

    fn key(account: &GithubAccountId) -> Option<ObservationKey> {
        Some(ObservationKey::InstalledOn { account: account.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.installed_on
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
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

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
