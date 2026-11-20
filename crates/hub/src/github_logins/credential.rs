//! What every worker gives the processes it starts for github.com, worked out from the default GitHub account in one
//! place, under the logins' lock, and kept as it changes for each worker's link and the check of reach to follow, and
//! recorded without its token on the hub's settings, for every client to show.
//! `docs/decisions/git-credential-set-by-hub.md` says why the hub sets it, and `docs/principles/outside-systems.md`
//! why a default that can't be used blocks.

use protocol::client::ApiError;
use protocol::model::{
    GithubAccount, GithubAccountId, GithubInstallation, GithubLoginKind, GithubToken, GithubUser, HubSettings,
    InstalledApp, SentCredential, UnusableLogin,
};
use protocol::worker::{GitCredential, GitIdentity};
use tokio::sync::watch;
use tracing::{error, info};

use super::blocks::{Unusable, lost, reason};
use super::{DefaultCredential, GithubLogin, GithubLogins};
use crate::login_files::{Locked, Stored};
use crate::store::StoreError;
use github::GithubProfile;

/// Who git says it is as it gives GitHub an installation's token, as GitHub's docs give.
const INSTALLATION_USERNAME: &str = "x-access-token";

impl GithubLogins {
    /// What workers are to give their processes for github.com, from now on, as it changes, with who GitHub knows the
    /// default as, as [`DefaultCredential`] says. It changes under the logins' lock, as each change to the logins is
    /// made, so it changes in the order they were made. Each worker's link follows it on a task of its own, so nothing
    /// is sent to a worker under the lock, and no change to the logins waits on a worker. *Unconfirmed:* a `watch`.
    pub(crate) fn git_credential(&self) -> watch::Receiver<DefaultCredential> {
        self.credential.subscribe()
    }

    /// The hub's settings each time what workers are sent was recorded anew on them, for the hub to tell every client
    /// of, from now on. Only the newest is kept, which is all a client takes, as a copy no newer than its own is
    /// dropped. `None` until it's first recorded after this is called.
    pub(crate) fn recorded_settings(&self) -> watch::Receiver<Option<HubSettings>> {
        self.recorded.subscribe()
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
            Ok(Stored::Secret(GithubLogin::Gh { user })) => self.gh_token(user, now_ms),
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

    /// Has workers sent what they're to give their processes at `now_ms`, and records it, without its token, on the
    /// hub's settings, each if that has changed.
    pub(super) fn publish(&self, files: &Locked<'_, GithubLogin>, now_ms: i64) {
        let credential = self.worked_out(files, now_ms);
        if let Some(sent) = credential.recorded() {
            self.record_sent(&sent);
        }
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

    /// Records `sent` on the hub's settings, and has them told of as [`GithubLogins::recorded_settings`] says, when
    /// that changed them. One that can't be recorded leaves the last, until the next change, or the next check within a
    /// minute, records it, and workers are sent it all the same.
    fn record_sent(&self, sent: &SentCredential) {
        match self.store.record_github_sent(sent) {
            Ok(Some(settings)) => {
                self.recorded.send_replace(Some(settings));
            }
            Ok(None) => {}
            Err(e) => error!("couldn't record what workers are sent for github.com on the hub's settings: {e}"),
        }
    }

    /// What workers are to give their processes for github.com at `now_ms`, with who GitHub knows the default as, as
    /// [`DefaultCredential`] says: nothing while the default is each machine's own login, the default's username and
    /// token while its login can be used, with its commit identity, an installation's bot, or as GitHub's profile of a
    /// token's or a sign-in's user said, once the hub has asked, and otherwise a block that says what to do, and why,
    /// never the machine's own in its place. So is a default the hub can't read, one whose profile it can't read, and an
    /// installation whose app's record it can't read. *Unconfirmed:* blocking for what the hub can't read.
    fn worked_out(&self, files: &Locked<'_, GithubLogin>, now_ms: i64) -> DefaultCredential {
        let unread = |e: &dyn std::fmt::Display| {
            reason(
                &format!("Roundhouse couldn't read its default GitHub account ({e})"),
                "it can",
                "It tries again within a minute.",
            )
        };
        let default = match self.store.hub_settings() {
            Ok(settings) => settings.github.and_then(|github| github.account),
            Err(e) => {
                return DefaultCredential::Blocked {
                    reason: unread(&e),
                    account: None,
                    why: UnusableLogin::Unreadable,
                };
            }
        };
        let Some(id) = default else { return DefaultCredential::MachineOwn };
        let blocked = |why, reason| DefaultCredential::Blocked { reason, account: Some(id.clone()), why };
        let account = match self.store.github_account(&id) {
            Ok(Some(account)) => account,
            Ok(None) => {
                let why = "Roundhouse's default GitHub account is one it has no record of";
                return blocked(UnusableLogin::Unreadable, reason(why, "another is chosen", "Choose another default."));
            }
            Err(e) => return blocked(UnusableLogin::Unreadable, unread(&e)),
        };
        let token = match self.token_of(files, &account, now_ms) {
            Ok(Ok(token)) => token,
            Ok(Err(unusable)) => return blocked(unusable.said(), lost(&account, &unusable)),
            Err(e) => return blocked(UnusableLogin::Unreadable, unread(&e.message)),
        };
        if let Some(installation) = &account.installation {
            return match self.installed_app(installation) {
                Ok(Some(installed)) => installation_credential(account.id, account.user, installed, token),
                // Removing an app forgets its installations' logins first, so only a store changed behind the hub's
                // back has one whose login can be used without the app.
                Ok(None) => {
                    let why = format!(
                        "Roundhouse has no record of the app its default GitHub account, {}, is an installation of",
                        account.user.name,
                    );
                    blocked(UnusableLogin::Unreadable, reason(&why, "another is chosen", "Choose another default."))
                }
                Err(e) => blocked(UnusableLogin::Unreadable, unread(&e)),
            };
        }
        match self.store.github_profile(&account.id) {
            Ok(profile) => {
                let identity = profile.map(|profile| identity_of(&account.user, profile));
                let credential = GitCredential { username: account.user.name.clone(), token, identity };
                let username = account.user.name;
                DefaultCredential::Account { account: account.id, credential, username, installation: None }
            }
            Err(e) => blocked(UnusableLogin::Unreadable, unread(&e)),
        }
    }

    /// Where `installation`'s app is installed, as a reach says of it, unless the hub has no record of the app.
    fn installed_app(&self, installation: &GithubInstallation) -> Result<Option<InstalledApp>, StoreError> {
        let app = self.store.github_app(installation.app)?;
        Ok(app.map(|app| InstalledApp { slug: app.slug, on: installation.on.name.clone() }))
    }
}

/// What git and `gh` are given for an app's installation, `account`, whose user is the app's `bot`, installed as
/// `installed` says, and whose login's token is `token`: `x-access-token` as its username, as GitHub's docs give git
/// for an installation's token, and the bot as the commit identity, so commits, pushes and pull requests all say the
/// bot. A bot has no profile, so it's its name and the address GitHub links to it. GitHub knows the installation as the
/// bot.
fn installation_credential(
    account: GithubAccountId,
    bot: GithubUser,
    installed: InstalledApp,
    token: GithubToken,
) -> DefaultCredential {
    let identity = identity_of(&bot, GithubProfile::default());
    let credential = GitCredential { username: INSTALLATION_USERNAME.into(), token, identity: Some(identity) };
    DefaultCredential::Account { account, credential, username: bot.name, installation: Some(installed) }
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
