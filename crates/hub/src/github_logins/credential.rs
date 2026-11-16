//! What every worker gives the processes it starts for github.com, worked out from the default GitHub account in one
//! place, under the logins' lock, and kept as it changes for each worker's link to follow, as "The worker's git
//! credential" in `docs/architecture.md` says.

use std::io;

use protocol::client::ApiError;
use protocol::model::{GithubAccount, GithubLoginKind, GithubToken, GithubUsername};
use protocol::worker::{GitCredential, GithubCredential};
use tokio::sync::watch;
use tracing::info;

use super::{GithubLogin, GithubLogins};
use crate::login_files::{Locked, Stored};

/// Why the login the hub holds for an account can't be used now.
pub(super) enum Unusable {
    /// It holds none, as once it was disconnected.
    Disconnected,
    /// Its token has ended, by GitHub's word.
    Ended,
    /// It's a sign-in's, whose token has ended, by GitHub's word, or that needs signing in again, as when GitHub has
    /// refused what renews it, so only signing in again gives it a login that works.
    SignInAgain,
    /// It's a sign-in's whose token has ended, by GitHub's word, which what renews it still can, as the hub tries to at
    /// each check, so it works again once that's done, as after a Mac wakes from a sleep past the token's end.
    Renewing,
    /// Its file holds a login in a form only a newer build reads, so this one can't use it, and keeps it for that one.
    Newer,
    /// Its file holds no token, or, with the error, can't be read.
    Unreadable(Option<io::Error>),
}

impl GithubLogins {
    /// What workers are to give their processes for github.com, from now on, as it changes. It changes under the
    /// logins' lock, as each change to the logins is made, so it changes in the order they were made.
    pub(crate) fn git_credential(&self) -> watch::Receiver<GithubCredential> {
        self.credential.subscribe()
    }

    /// Runs `change` under the logins' lock, then, still under it, works out what workers are to be sent at `now_ms`,
    /// whether `change` worked or not, since one that failed part way can have changed something.
    pub(super) fn changing<T>(
        &self,
        now_ms: i64,
        change: impl FnOnce(&Locked<'_, GithubLogin>) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        let files = self.files.lock();
        let changed = change(&files);
        self.publish(&files, now_ms);
        changed
    }

    /// The token the hub holds for `account`, a token's or a sign-in's, or why it can't be used at `now_ms`.
    pub(super) fn token_of(
        &self,
        files: &Locked<'_, GithubLogin>,
        account: &GithubAccount,
        now_ms: i64,
    ) -> Result<Result<GithubToken, Unusable>, ApiError> {
        let Some(held) = account.held_login else { return Ok(Err(Unusable::Disconnected)) };
        if !held.works_at(now_ms) {
            let unusable = if held.needs_sign_in {
                Unusable::SignInAgain
            } else if held.how == GithubLoginKind::SignIn {
                self.ended_sign_in(files, account, now_ms)?
            } else {
                Unusable::Ended
            };
            return Ok(Err(unusable));
        }
        let Some(file) = self.store.github_login_file(&account.id)? else { return Ok(Err(Unusable::Disconnected)) };
        Ok(match files.read(&file) {
            Ok(Stored::Secret(login)) => Ok(login.token().clone()),
            Ok(Stored::Newer) => Err(Unusable::Newer),
            Ok(Stored::Nothing) => Err(Unusable::Unreadable(None)),
            Err(e) => Err(Unusable::Unreadable(Some(e))),
        })
    }

    /// Why `account`, a sign-in whose token has ended, can't be used at `now_ms`: it's being renewed while what renews
    /// it still can, and otherwise only signing in again gives it a login that works.
    fn ended_sign_in(
        &self,
        files: &Locked<'_, GithubLogin>,
        account: &GithubAccount,
        now_ms: i64,
    ) -> Result<Unusable, ApiError> {
        let Some(file) = self.store.github_login_file(&account.id)? else { return Ok(Unusable::Disconnected) };
        let renewable = matches!(
            files.read(&file),
            Ok(Stored::Secret(GithubLogin::SignIn(signed_in))) if signed_in.renews_with(now_ms).is_some()
        );
        Ok(if renewable { Unusable::Renewing } else { Unusable::SignInAgain })
    }

    /// Has workers sent what they're to give their processes at `now_ms`, if that has changed.
    pub(super) fn publish(&self, files: &Locked<'_, GithubLogin>, now_ms: i64) {
        let credential = self.worked_out(files, now_ms);
        self.credential.send_if_modified(|current| {
            if *current == credential {
                return false;
            }
            // Its `Debug` leaves out the token. As the hub starts, too, so its log says what workers are sent from then.
            info!(?credential, "what workers give their processes for github.com from now on");
            *current = credential;
            true
        });
    }

    /// What workers are to give their processes for github.com at `now_ms`: nothing while the default is each
    /// machine's own login, the default's username and token while its login can be used, and otherwise a block that
    /// says what to do, never the machine's own in its place. So is a default the hub can't read.
    fn worked_out(&self, files: &Locked<'_, GithubLogin>, now_ms: i64) -> GithubCredential {
        let unread = |e: &dyn std::fmt::Display| {
            let why = format!("Roundhouse couldn't read its default GitHub account ({e})");
            blocked(&why, "It tries again within a minute.")
        };
        let default = match self.store.hub_settings() {
            Ok(settings) => settings.github.and_then(|github| github.account),
            Err(e) => return unread(&e),
        };
        let Some(id) = default else { return GithubCredential::MachineOwn };
        let account = match self.store.github_account(&id) {
            Ok(Some(account)) => account,
            Ok(None) => {
                let why = "Roundhouse's default GitHub account is one it has no record of";
                return blocked(why, "Choose another default in Roundhouse's Settings.");
            }
            Err(e) => return unread(&e),
        };
        match self.token_of(files, &account, now_ms) {
            Ok(Ok(token)) => GithubCredential::Account(GitCredential { username: account.user.name, token }),
            Ok(Err(unusable)) => lost(&account.user.name, &unusable),
            Err(e) => unread(&e.message),
        }
    }
}

/// A block, saying `why` agents push as nobody, and `to_do` about it. Git says it as it refuses to push, in a terminal
/// or an agent's output, where nothing else says it's Roundhouse's, so it names Roundhouse.
fn blocked(why: &str, to_do: &str) -> GithubCredential {
    GithubCredential::Blocked { reason: format!("{why}, so this workspace's agents push to GitHub as nobody. {to_do}") }
}

/// The block for a default, `name`, whose login can't be used, as `unusable` says.
fn lost(name: &GithubUsername, unusable: &Unusable) -> GithubCredential {
    let connect_again = "Connect it again in Roundhouse's Settings, or choose another default there.";
    match unusable {
        Unusable::Disconnected => {
            blocked(&format!("Roundhouse's default GitHub account, {name}, was disconnected"), connect_again)
        }
        Unusable::Ended => blocked(
            &format!("The token of Roundhouse's default GitHub account, {name}, has ended"),
            "Renew it in Roundhouse's Settings, or choose another default there.",
        ),
        Unusable::SignInAgain => blocked(
            &format!("The sign-in of Roundhouse's default GitHub account, {name}, has ended"),
            "Sign in with GitHub again in Roundhouse's Settings, or choose another default there.",
        ),
        Unusable::Renewing => blocked(
            &format!(
                "The token of Roundhouse's default GitHub account, {name}, has ended, and Roundhouse is renewing it"
            ),
            "It tries every minute until GitHub answers. If this goes on, sign in with GitHub again in Roundhouse's \
             Settings.",
        ),
        Unusable::Newer => blocked(
            &format!(
                "The login of Roundhouse's default GitHub account, {name}, is one only a newer Roundhouse can read"
            ),
            "Update Roundhouse, connect the account again in its Settings, or choose another default there.",
        ),
        Unusable::Unreadable(None) => {
            blocked(&format!("Roundhouse can't find the token of its default GitHub account, {name}"), connect_again)
        }
        Unusable::Unreadable(Some(e)) => blocked(
            &format!("Roundhouse can't read the token of its default GitHub account, {name} ({e})"),
            connect_again,
        ),
    }
}
