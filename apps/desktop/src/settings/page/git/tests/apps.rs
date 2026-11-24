//! What the page says of apps of your own: the name an app is first given, how an installation's login stands, as its
//! record and the hub say, and which refusals are worth trying again.

use client_core::ClientError;
use gpui_kit::TestAppContext;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    GithubAccount, GithubAccountId, GithubAppId, GithubInstallation, GithubInstallationId, GithubLoginKind,
    Observation, Records, SentCredential, UnusableLogin,
};

use super::super::app_words;
use super::super::words::{self, Standing};
use super::made_up::{NOW_MS, held, listed, named};

/// `name`'s account, `id`, as an installation of the app 9 on GitHub's user 2, and what GitHub calls its user.
pub(super) fn installed(id: &str, name: &str) -> (GithubAccount, Observation) {
    let on = protocol::fixtures::github_user(2);
    let installation = GithubInstallation { app: GithubAppId::new(9), id: GithubInstallationId::new(70), on };
    let (account, username) = named(id, name, false);
    let held_login = Some(held(GithubLoginKind::App, None, false));
    (GithubAccount { held_login, installation: Some(installation), ..account }, username)
}

#[test]
fn an_app_is_first_named_after_someone_added() {
    let named_for = |accounts: Vec<(GithubAccount, Observation)>, default: Option<&str>| {
        let (github_accounts, observations) = accounts.into_iter().unzip();
        let listing = Records { github_accounts, observations, ..Records::default() };
        app_words::default_name(&listed(listing, default), default.map(GithubAccountId::from).as_ref())
    };
    let bot = installed("g0", "ada-roundhouse[bot]");

    let added = vec![bot.clone(), named("g1", "bea", false), named("g2", "ada", false)];
    assert_eq!(named_for(added.clone(), Some("g2")), "ada-roundhouse", "the default first");
    assert_eq!(named_for(added, Some("g0")), "ada-roundhouse", "but never a bot");
    assert_eq!(named_for(vec![bot], Some("g0")), "", "with nobody to name it after");
}

/// Its record says nothing of when its token ends, as the hub keeps it, so only the hub can say it's making the next,
/// which it says only of the default, as what workers are sent.
#[gpui_kit::test]
fn an_installations_record_says_it_works_while_the_hub_holds_it_and_only_the_hub_says_its_renewing(
    cx: &mut TestAppContext,
) {
    let held = held(GithubLoginKind::App, None, false);
    let (of_its_record, as_the_default) = cx.update(|cx| {
        look::hold_still(NOW_MS, cx);
        let renewing =
            SentCredential::Blocked { account: "g0".into(), why: UnusableLogin::Renewing, reason: "Made up.".into() };
        (Standing::of(Some(&held), NOW_MS, cx), Standing::sent(&renewing, Some(&held), cx))
    });

    assert_eq!(of_its_record, Standing::Works { ends: None });
    assert_eq!(as_the_default, Standing::Renewing);
    assert_eq!(words::connected_by(GithubLoginKind::App), "App");
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
    assert_eq!(app_words::not_looked(&api(ErrorCode::NotFound)), app_words::gone());
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
