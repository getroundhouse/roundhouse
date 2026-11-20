//! An app of your own, `ada-roundhouse`, made under the organization `adalabs`, as Git in Settings lists it: installed
//! there, as the default, and as the default whose token has ended while the hub makes the next, and installed nowhere
//! yet. Its key and its installation's tokens are the hub's, so none is here.

use protocol::client::{ApiError, ErrorCode};
use protocol::fixtures::github_app;
use protocol::model::{
    GithubAccount, GithubApp, GithubAppId, GithubInstallation, GithubInstallationId, GithubLoginKind, GithubUser,
    GithubUserId, HeldGithubLogin, UnusableLogin,
};

use super::github::{blocking, github_accounts, sending};
use client_core::fake_hub::{HOUR_MS, NOW_MS, World};

/// [`github_accounts`], with the app installed on `adalabs`, made two days ago, whose installation is the default.
pub(in crate::gallery) fn github_app_installed() -> World {
    let mut world = github_accounts();
    let bot = installation();
    world.github = Some(sending(&bot));
    world.github_accounts.push(bot);
    world.github_apps = vec![app()];
    world
}

/// [`github_app_installed`], whose installation's last token has ended while GitHub won't make the next, so the hub
/// blocks every worker, which only it can say, since the installation's record says nothing of its token's end.
pub(in crate::gallery) fn github_app_renewing() -> World {
    let mut world = github_app_installed();
    let reason = "The token of ada-roundhouse[bot], the default account, installed on adalabs, has ended, and \
                  Roundhouse is making another. Until it has, Roundhouse can't read pull requests, and agents on the \
                  providers it sets up can't push. It tries every minute until GitHub answers. If this goes on, check \
                  the app is still on GitHub, or choose another default.";
    world.github = Some(blocking(&installation(), UnusableLogin::Renewing, reason));
    world
}

/// [`github_accounts`], with the app made, but installed nowhere yet.
pub(in crate::gallery) fn github_app_not_installed() -> World {
    let mut world = github_accounts();
    world.github_apps = vec![app()];
    world
}

/// [`github_app_not_installed`], whose hub can't reach GitHub to look for where it's installed.
pub(in crate::gallery) fn github_app_not_looked() -> World {
    let mut world = github_app_not_installed();
    world.app_look_refused = Some(ApiError::new(ErrorCode::Unavailable, "GitHub didn't answer in time"));
    world
}

/// The organization the app is made under, and installed on.
fn adalabs() -> GithubUser {
    GithubUser { id: GithubUserId::new(7), name: "adalabs".into() }
}

/// `ada-roundhouse`, under `adalabs`, as the panel names it by default for `ada`.
fn app() -> GithubApp {
    GithubApp { name: "ada-roundhouse".to_string(), owner: adalabs(), ..github_app(9) }
}

/// The app's installation on `adalabs`, as an account whose user is the app's bot, whose token the hub makes from the
/// app's key, an hour at a time, so its record says no end, as the hub keeps it.
fn installation() -> GithubAccount {
    let bot = app().bot.unwrap();
    GithubAccount {
        id: "github-0004".into(),
        user: bot,
        held_login: Some(HeldGithubLogin {
            how: GithubLoginKind::App,
            connected_at_ms: NOW_MS - 48 * HOUR_MS,
            ends_at_ms: None,
            needs_sign_in: false,
        }),
        installation: Some(GithubInstallation {
            app: GithubAppId::new(9),
            id: GithubInstallationId::new(50_001),
            on: adalabs(),
        }),
        rev: protocol::model::Rev::default(),
    }
}
