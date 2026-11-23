//! The kinds of what GitHub says of what the hub holds of GitHub: whether its default account can push to each
//! repository, and who each GitHub account it holds a login for is.

use protocol::model::{
    GithubAccountId, GithubReach, Observation, ObservationKey, Observed, Reach, Record, RecordKey, RepoId, Rev,
};

use super::kept::Table;
use super::kinds::{Form, Kind};
use super::tables::Tables;

/// Whether the default GitHub account can push to a repository's repository on GitHub, as GitHub last answered the
/// hub's check, or as the hub says without asking, sent at the hub's level. A client of a version from before it reads
/// it as the record `GithubReach`, made from it.
pub(crate) struct RepositoryReach;

impl Kind for RepositoryReach {
    type Of = RepoId;
    type Value = Reach;
    const FORM: Form = Form { table: "repository_reach", version: 1 };

    fn observation(repo: &RepoId, reach: Observed<Reach>, rev: Rev) -> Option<Observation> {
        Some(Observation::RepositoryReach { repo: repo.clone(), reach, rev })
    }

    fn key(repo: &RepoId) -> Option<ObservationKey> {
        Some(ObservationKey::RepositoryReach { repo: repo.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.repository_reach
    }

    /// GitHub's answer is as of when it was read, and what the hub says without asking it has no time. One that isn't
    /// current is from before this run's checks, as one the cache file kept, which the record calls stale.
    /// *Unconfirmed:* `answered_at_ms` only of the read that found what it says, not of each read since.
    fn old_record(repo: &RepoId, said: Observed<Reach>, read_at_ms: i64, rev: Rev) -> Option<Record> {
        let (reach, stale) = match said {
            Observed::Current { value } => (value, false),
            Observed::AsOf { value, .. } => (value, true),
            Observed::NotKnown | Observed::Unknown => return None,
        };
        let answered_at_ms = reach.answered_for().is_some().then_some(read_at_ms);
        Some(Record::GithubReach(GithubReach { repo: repo.clone(), reach, answered_at_ms, stale, rev }))
    }

    fn old_record_key(repo: &RepoId) -> Option<RecordKey> {
        Some(RecordKey::GithubReach { repo: repo.clone() })
    }
}

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
