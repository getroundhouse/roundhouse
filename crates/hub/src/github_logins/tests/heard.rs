//! What the hub does around the logins as it hears who a GitHub account is, which keeps it as observations, for the
//! tests here, which have no hub.

use super::*;
use crate::hub::{GithubProfile, GithubUsername, Observations};
use protocol::model::GithubAppId;

/// What the hub does once the logins have connected `account` as `owner`, at `at_ms`: keeps what GitHub said of the user,
/// as the hub keeps it as observations, and has workers sent what that makes of the default.
pub(crate) fn heard(logins: &GithubLogins, account: &GithubAccountId, owner: &TokenOwner, at_ms: i64) {
    let observations = &logins.observations;
    observations.keep_as_read::<GithubUsername>(account.clone(), owner.user.name.clone(), at_ms);
    observations.keep_as_read::<GithubProfile>(account.clone(), owner.profile.clone(), at_ms);
    logins.heard_of(at_ms).unwrap();
}

/// The logins of a hub whose data directory is `dir`, started at `now_ms`, over `store`.
pub(crate) fn logins_over(store: &Arc<Store>, dir: &std::path::Path, now_ms: i64) -> GithubLogins {
    GithubLogins::new(Arc::clone(store), Observations::in_a_test(dir, store), dir, now_ms)
}

impl GithubLogins {
    /// Connects as the hub does, keeping what GitHub said of the user, as [`heard`] says.
    pub(crate) fn connected(
        &self,
        connect: &Connect,
        owner: &TokenOwner,
    ) -> Result<GithubAccountChange, protocol::client::ApiError> {
        let change = self.connect(connect, owner)?;
        heard(self, &change.account.id, owner, connect.connected_at_ms);
        Ok(change)
    }

    /// Approves a sign-in as the hub does, keeping what GitHub said of the user, as [`heard`] says.
    pub(crate) fn approved_as(
        &self,
        approved: &Approved,
        owner: &TokenOwner,
    ) -> Result<SignInChange, protocol::client::ApiError> {
        let change = self.approve_sign_in(approved, owner)?;
        if let Some(account) = &change.account {
            heard(self, &account.account.id, owner, approved.signed_in_at_ms);
        }
        Ok(change)
    }

    /// Connects an app's installation as the hub does, keeping that its user is the app's bot, `bot`, and what GitHub
    /// calls what it's on.
    pub(crate) fn installation_connected(
        &self,
        app: GithubAppId,
        bot: &GithubUser,
        installation: &github::Installation,
        token: InstallationToken,
        by_person: bool,
        now_ms: i64,
    ) -> Result<Option<GithubAccountChange>, protocol::client::ApiError> {
        let change = self.connect_installation(app, bot, installation, token, by_person, now_ms)?;
        if let Some(change) = &change {
            let account = change.account.id.clone();
            self.observations.keep_as_read::<GithubUsername>(account.clone(), bot.name.clone(), now_ms);
            self.observations.keep_as_read::<crate::hub::InstalledOn>(account, installation.on.name.clone(), now_ms);
            self.heard_of(now_ms)?;
        }
        Ok(change)
    }
}
