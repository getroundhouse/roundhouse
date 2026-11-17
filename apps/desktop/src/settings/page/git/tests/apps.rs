//! What the page says of apps of your own: the name an app is first given, how an installation's login stands, and
//! which refusals are worth trying again.

use client_core::ClientError;
use gpui_kit::TestAppContext;
use protocol::client::{ApiError, ErrorCode};
use protocol::fixtures::found_github_login;
use protocol::model::{
    FoundGithubLogin, GhLogin, GhLoginWorks, GithubAccount, GithubAccountId, GithubAppId, GithubInstallation,
    GithubInstallationId, GithubLoginKind, GithubUser, GithubUserId, Records,
};

use super::super::app_words;
use super::super::words::{self, Standing};
use super::made_up::{HOUR_MS, NOW_MS, held, listed, named};

/// `name`'s account, `id`, as an installation of the app 9 on adalabs.
pub(super) fn installed(id: &str, name: &str) -> GithubAccount {
    let on = GithubUser { id: GithubUserId::new(2), name: "adalabs".into() };
    let installation = GithubInstallation { app: GithubAppId::new(9), id: GithubInstallationId::new(70), on };
    GithubAccount {
        held_login: Some(held(GithubLoginKind::App, None, false)),
        installation: Some(installation),
        ..named(id, name, false)
    }
}

#[test]
fn an_app_is_first_named_after_someone_added_or_else_after_who_gh_is_logged_in_as() {
    let named_for = |accounts, found, default: Option<&str>| {
        let listing = Records { github_accounts: accounts, found_github_logins: found, ..Records::default() };
        app_words::default_name(&listed(listing, default), default.map(GithubAccountId::from).as_ref())
    };
    let bot = installed("g0", "ada-roundhouse[bot]");
    let logged_in = |name: &str| FoundGithubLogin {
        login: GhLogin::LoggedIn { username: name.into(), works: GhLoginWorks::Yes },
        ..found_github_login("local")
    };

    let added = vec![bot.clone(), named("g1", "bea", false), named("g2", "ada", false)];
    assert_eq!(named_for(added.clone(), Vec::new(), Some("g2")), "ada-roundhouse", "the default first");
    let both = named_for(added.clone(), vec![logged_in("grace")], Some("g1"));
    assert_eq!(both, "bea-roundhouse", "someone added before who gh is logged in as");
    assert_eq!(named_for(added, Vec::new(), Some("g0")), "ada-roundhouse", "but never a bot");
    assert_eq!(named_for(vec![bot], vec![logged_in("grace")], Some("g0")), "grace-roundhouse");
    let logged_out = FoundGithubLogin { login: GhLogin::LoggedOut, ..found_github_login("local") };
    assert_eq!(named_for(Vec::new(), vec![logged_out], None), "", "with nobody to name it after");
}

#[gpui_kit::test]
fn an_installations_login_says_nothing_of_its_end_since_the_hub_makes_the_next(cx: &mut TestAppContext) {
    let standing = |ends_at_ms| {
        let held = held(GithubLoginKind::App, ends_at_ms, false);
        cx.update(|cx| {
            look::hold_still(NOW_MS, cx);
            Standing::of(Some(&held), NOW_MS, cx)
        })
    };

    assert_eq!(standing(None), Standing::Works { ends: None });
    assert_eq!(standing(Some(NOW_MS + HOUR_MS)), Standing::Works { ends: None }, "from a hub that says one");
    assert_eq!(standing(Some(NOW_MS - HOUR_MS)), Standing::Renewing, "and the hub makes the next once it has ended");
    assert_eq!(words::connected_by(GithubLoginKind::App), "App");
}

#[test]
fn what_stops_a_default_installation_says_what_to_do_as_its_app_stands() {
    assert!(app_words::blocked("ada-roundhouse[bot]", true, true).contains("making its next from the app's key"));
    assert!(app_words::blocked("ada-roundhouse[bot]", false, true).contains("Connect it again from its menu"));
    assert_eq!(
        app_words::blocked("ada-roundhouse[bot]", false, false),
        "The default, ada-roundhouse[bot], is of an app that was removed. Make another account the default."
    );
}

#[test]
fn a_code_github_no_longer_takes_starts_again_and_anything_else_tries_the_same_code_again() {
    let api = |code| ClientError::Api(ApiError::new(code, "made up"));

    let (said, again) = app_words::not_made(&api(ErrorCode::InvalidRequest));
    assert!(said.starts_with("GitHub no longer takes the code") && !again);
    assert!(!app_words::not_made(&api(ErrorCode::Unsupported)).1);
    let (said, again) = app_words::not_made(&api(ErrorCode::Unavailable));
    assert_eq!(
        said,
        "The hub couldn't take the app's key from GitHub: made up. GitHub takes its code for an hour, so try again."
    );
    assert!(again);
    assert!(app_words::not_made(&ClientError::Disconnected).1, "the hub unreachable");
    assert_eq!(app_words::not_looked(&api(ErrorCode::NotFound)), app_words::GONE);
}

#[test]
fn a_name_github_takes_for_an_organization_is_asked_for_by_what_was_typed() {
    let not = |typed: &str| app_words::not_an_organization(&crate::manifest_flow::NotAnOrganization(typed.to_string()));

    assert_eq!(not(""), "Type the organization's name.");
    assert_eq!(
        not("ada labs"),
        "ada labs isn't an organization's name. GitHub's have only letters, numbers and single hyphens."
    );
}
