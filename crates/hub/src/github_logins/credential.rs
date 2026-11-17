//! What every worker gives the processes it starts for github.com, worked out from the default GitHub account in one
//! place, under the logins' lock, and kept as it changes for each worker's link and the check of reach to follow, as
//! "The worker's git credential" in `docs/architecture.md` says.

use std::io;

use protocol::client::ApiError;
use protocol::model::{
    GithubAccount, GithubInstallation, GithubLoginKind, GithubToken, GithubUser, GithubUsername, InstalledApp,
};
use protocol::worker::{GitCredential, GitIdentity};
use tokio::sync::watch;
use tracing::info;

use super::{DefaultCredential, GithubLogin, GithubLogins};
use crate::login_files::{Locked, Stored};
use crate::store::{GithubProfile, StoreError};

/// Who git says it is as it gives GitHub an installation's token, as GitHub's docs give.
const INSTALLATION_USERNAME: &str = "x-access-token";

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
    /// What workers are to give their processes for github.com, from now on, as it changes, with who GitHub knows the
    /// default as, as [`DefaultCredential`] says. It changes under the logins' lock, as each change to the logins is
    /// made, so it changes in the order they were made.
    pub(crate) fn git_credential(&self) -> watch::Receiver<DefaultCredential> {
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

    /// The token the hub holds for `account`, a token's, a sign-in's or an installation's, or why it can't be used at
    /// `now_ms`. An installation's record doesn't say when its token ends, as
    /// [`HeldGithubLogin::ends_at_ms`](protocol::model::HeldGithubLogin::ends_at_ms) says, so its file does.
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
            Ok(Stored::Secret(GithubLogin::Installation(made))) if made.token_ends_at_ms <= now_ms => {
                Err(Unusable::Renewing)
            }
            Ok(Stored::Secret(login)) => login.token().cloned().ok_or(Unusable::Unreadable(None)),
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

    /// What workers are to give their processes for github.com at `now_ms`, with who GitHub knows the default as, as
    /// [`DefaultCredential`] says: nothing while the default is each machine's own login, the default's username and
    /// token while its login can be used, with its commit identity, an installation's bot, or as GitHub's profile of a
    /// token's or a sign-in's user said, once the hub has asked, and otherwise a block that says what to do, never the
    /// machine's own in its place. So is a default the hub can't read, one whose profile it can't read, and an
    /// installation whose app's record it can't read.
    fn worked_out(&self, files: &Locked<'_, GithubLogin>, now_ms: i64) -> DefaultCredential {
        let unread = |e: &dyn std::fmt::Display| {
            let why = format!("Roundhouse couldn't read its default GitHub account ({e})");
            blocked(&why, "It tries again within a minute.")
        };
        let default = match self.store.hub_settings() {
            Ok(settings) => settings.github.and_then(|github| github.account),
            Err(e) => return unread(&e),
        };
        let Some(id) = default else { return DefaultCredential::MachineOwn };
        let account = match self.store.github_account(&id) {
            Ok(Some(account)) => account,
            Ok(None) => {
                let why = "Roundhouse's default GitHub account is one it has no record of";
                return blocked(why, "Choose another default in Roundhouse's Settings.");
            }
            Err(e) => return unread(&e),
        };
        let token = match self.token_of(files, &account, now_ms) {
            Ok(Ok(token)) => token,
            Ok(Err(unusable)) => return lost(&account, &unusable),
            Err(e) => return unread(&e.message),
        };
        if let Some(installation) = &account.installation {
            return match self.installed_app(installation) {
                Ok(Some(installed)) => installation_credential(account.user, installed, token),
                // Removing an app forgets its installations' logins first, so only a store changed behind the hub's
                // back has one whose login can be used without the app.
                Ok(None) => {
                    let why = format!(
                        "Roundhouse has no record of the app its default GitHub account, {}, is an installation of",
                        account.user.name,
                    );
                    blocked(&why, "Choose another default in Roundhouse's Settings.")
                }
                Err(e) => unread(&e),
            };
        }
        match self.store.github_profile(&account.id) {
            Ok(profile) => {
                let identity = profile.map(|profile| identity_of(&account.user, profile));
                let credential = GitCredential { username: account.user.name.clone(), token, identity };
                DefaultCredential::Account { credential, username: account.user.name, installation: None }
            }
            Err(e) => unread(&e),
        }
    }

    /// Where `installation`'s app is installed, as a reach says of it, unless the hub has no record of the app.
    fn installed_app(&self, installation: &GithubInstallation) -> Result<Option<InstalledApp>, StoreError> {
        let app = self.store.github_app(installation.app)?;
        Ok(app.map(|app| InstalledApp { slug: app.slug, on: installation.on.name.clone() }))
    }
}

/// What git and `gh` are given for an app's installation, whose user is the app's `bot`, installed as `installed`
/// says, and whose login's token is `token`: `x-access-token` as its username, as GitHub's docs give git for an
/// installation's token, and the bot as the commit identity, so commits, pushes and pull requests all say the bot. A
/// bot has no profile, so it's its name and the address GitHub links to it. GitHub knows the installation as the bot.
fn installation_credential(bot: GithubUser, installed: InstalledApp, token: GithubToken) -> DefaultCredential {
    let identity = identity_of(&bot, GithubProfile::default());
    let credential = GitCredential { username: INSTALLATION_USERNAME.into(), token, identity: Some(identity) };
    DefaultCredential::Account { credential, username: bot.name, installation: Some(installed) }
}

/// Who commits are by as `user`, as its `profile` says: the name it goes by, or its username when it has none, or one
/// git won't commit as, as [`git_commits_as`] says, and the email it shows everyone, or else the address GitHub gives
/// it, `<number>+<username>@users.noreply.github.com`, which GitHub links to the user whatever it's named later, and
/// which no one can mail, as the owner decided. Never the primary email a sign-in can read, since GitHub refuses a push
/// of commits by one the person keeps private when they've asked it to.
fn identity_of(user: &GithubUser, profile: GithubProfile) -> GitIdentity {
    let GithubUser { id, name: username } = user;
    GitIdentity {
        name: profile.name.filter(|name| git_commits_as(name)).unwrap_or_else(|| username.to_string()),
        email: profile.email.unwrap_or_else(|| format!("{id}+{username}@users.noreply.github.com")),
    }
}

/// Whether git commits as `name` as its `user.name`. Git takes `<`, `>` and line ends out of a name, trims what it
/// calls crud from both ends, and refuses to commit as one with nothing left, which is one with nothing but crud: each
/// byte up to a space, and `.`, `,`, `:`, `;`, `<`, `>`, `"`, `\` and `'`, as git 2.31, the oldest the worker uses,
/// has it. Later gits, such as 2.54, keep `.`, so a name of only dots, which they'd commit as, is the username on
/// every git.
fn git_commits_as(name: &str) -> bool {
    name.chars().any(|c| c > ' ' && !".,:;<>\"\\'".contains(c))
}

/// A block, saying `why` agents push as nobody, and `to_do` about it. Git says it as it refuses to push, in a terminal
/// or an agent's output, where nothing else says it's Roundhouse's, so it names Roundhouse.
fn blocked(why: &str, to_do: &str) -> DefaultCredential {
    DefaultCredential::Blocked {
        reason: format!("{why}, so this workspace's agents push to GitHub as nobody. {to_do}"),
    }
}

/// The block for a default, `account`, whose login can't be used, as `unusable` says. An app's installation is named
/// with where it's installed, and is connected again, and renewed, with the app.
fn lost(account: &GithubAccount, unusable: &Unusable) -> DefaultCredential {
    let Some(installation) = &account.installation else { return lost_login(&account.user.name, unusable) };
    let name = format!("{}, installed on {}", account.user.name, installation.on.name);
    let elsewhere = "or choose another default in Roundhouse's Settings.";
    match unusable {
        Unusable::Disconnected => blocked(
            &format!("Roundhouse's default GitHub account, {name}, was disconnected, or the app uninstalled there"),
            &format!(
                "Install the app there again, connect its installations again in Roundhouse's Settings, {elsewhere}"
            ),
        ),
        Unusable::Renewing => blocked(
            &format!(
                "The token of Roundhouse's default GitHub account, {name}, has ended, and Roundhouse is making another"
            ),
            &format!(
                "It tries every minute until GitHub answers. If this goes on, check the app is still on GitHub, {elsewhere}"
            ),
        ),
        Unusable::Ended | Unusable::SignInAgain | Unusable::Newer | Unusable::Unreadable(_) => {
            lost_login(&name.into(), unusable)
        }
    }
}

/// The block for a default, `name`, whose login can't be used, as `unusable` says.
fn lost_login(name: &GithubUsername, unusable: &Unusable) -> DefaultCredential {
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
