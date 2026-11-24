//! Asking GitHub with no login who an account is whose own login can't be asked with, as the hub's check does, against
//! a stand-in for GitHub that answers `GET /user/{number}` only when it's asked with no login. Which accounts are due,
//! and how often the hub asks so, is tested in `crate::github_logins`.

use std::sync::Arc;

use protocol::model::{GithubAccountId, GithubToken, GithubUser, GithubUserId, GithubUsername};

use super::*;
use crate::github_logins::Connect;
use crate::hub::Hub;
use crate::hub::tests::hub_calling_github;
use github::{Answer, GithubProfile, Script, StandIn, TokenOwner};

const HOUR_MS: i64 = 60 * 60 * 1000;

/// A hub calling `github`, with ada's token, `a-token`, connected at 1000, ending at `ends_at_ms`, and nothing kept of
/// who she is on GitHub, as on the first start after the hub stopped keeping it in its database.
fn hub_with_ada(dir: &std::path::Path, github: &StandIn, ends_at_ms: Option<i64>) -> Arc<Hub> {
    let hub = hub_calling_github(dir, github.api());
    connect(&hub, "a-token", 7, "ada", ends_at_ms);
    hub
}

/// Connects `token`, of the GitHub user `name`, numbered `user`, at 1000, ending at `ends_at_ms`, keeping nothing of
/// who the user is, and returns its account.
fn connect(hub: &Hub, token: &str, user: u64, name: &str, ends_at_ms: Option<i64>) -> GithubAccountId {
    let connect = Connect::new(GithubAccountConnect { token: GithubToken::new(token), id: None }, 1000).unwrap();
    let user = GithubUser { id: GithubUserId::new(user), name: name.into() };
    let owner = TokenOwner { user, profile: GithubProfile::default(), ends_at_ms };
    hub.github_logins.connect(&connect, &owner).unwrap().account.id
}

/// What the hub calls ada's account's user, if it knows.
fn named(hub: &Hub) -> Option<GithubUsername> {
    let account = hub.store.hub_settings().unwrap().github.and_then(|github| github.account).unwrap();
    hub.observation_value::<crate::hub::GithubUsername>(&account)
}

/// How many times GitHub was asked for user 7 by number.
fn asked_by_number(github: &StandIn) -> usize {
    asked_for(github, 7)
}

/// How many times GitHub was asked for the user numbered `user`.
fn asked_for(github: &StandIn, user: u64) -> usize {
    github.asked().iter().filter(|asked| asked.starts_with(&format!("get /user/{user} "))).count()
}

/// Ada's token ended long before the hub started, so it can't be asked with: the first check asks GitHub with no
/// login, and she's named, as a person reads her account, within that check.
#[tokio::test]
async fn an_account_whose_login_cant_be_used_is_named_by_github_asked_with_no_login() {
    let ada = Answer::user_with_profile(7, "ada", Some("Ada Lovelace"), None);
    let github = StandIn::with(Script { numbered: vec![(7, vec![ada])], ..Script::default() }).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_ada(dir.path(), &github, Some(2000));

    hub.check_github_logins(1_000_000).await;

    assert_eq!(named(&hub).as_ref().map(GithubUsername::as_str), Some("ada"));
    assert_eq!(asked_by_number(&github), 1);
}

/// GitHub limits the first ask with no login, which leaves her not known and decides nothing, and no check asks again
/// for an hour, after which one does and names her.
#[tokio::test]
async fn a_limited_ask_with_no_login_leaves_the_account_not_known_and_is_asked_again_an_hour_on() {
    let limited = Answer::status(403, vec![("x-ratelimit-remaining", "0")]);
    let ada = Answer::user(7, "ada", None);
    let github = StandIn::with(Script { numbered: vec![(7, vec![limited, ada])], ..Script::default() }).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_ada(dir.path(), &github, Some(2000));

    hub.check_github_logins(1_000_000).await;
    let after_the_limit = named(&hub);
    hub.check_github_logins(1_000_000 + HOUR_MS - 1).await;
    let asked_within_the_hour = asked_by_number(&github);
    hub.check_github_logins(1_000_000 + HOUR_MS).await;

    assert_eq!(after_the_limit, None, "not known");
    assert_eq!(asked_within_the_hour, 1, "nothing asked while GitHub limits it");
    assert_eq!(named(&hub).as_ref().map(GithubUsername::as_str), Some("ada"));
}

/// GitHub answers that it has no user 7, as once ada's user is deleted, so the next check, a minute on, doesn't ask
/// again, and a check a day on does. Grace's ask fails, so that next check asks again, and names her.
#[tokio::test]
async fn an_account_github_has_no_user_for_is_asked_again_a_day_on_and_one_whose_ask_failed_at_the_next_check() {
    let grace = vec![Answer::status(502, Vec::new()), Answer::user(8, "grace", None)];
    let github = StandIn::with(Script { numbered: vec![(8, grace)], ..Script::default() }).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_ada(dir.path(), &github, Some(2000));
    let grace = connect(&hub, "b-token", 8, "grace", Some(2000));

    hub.check_github_logins(1_000_000).await;
    hub.check_github_logins(1_000_000 + 60_000).await;
    let in_the_day = (asked_for(&github, 7), asked_for(&github, 8));
    hub.check_github_logins(1_000_000 + 24 * HOUR_MS).await;

    assert_eq!(in_the_day, (1, 2), "ada asked once, and grace again at the next check");
    let grace = hub.observation_value::<crate::hub::GithubUsername>(&grace);
    assert_eq!(grace.as_ref().map(GithubUsername::as_str), Some("grace"));
    assert_eq!(asked_by_number(&github), 2, "asked again a day on");
    assert_eq!(named(&hub), None, "ada still not known");
}

/// Ada's token works, so the check asks with it, and never with no login.
#[tokio::test]
async fn an_account_whose_login_works_is_never_asked_about_with_no_login() {
    let ada = || Answer::user(7, "ada", None);
    let script = Script { users: vec![("a-token", ada())], numbered: vec![(7, vec![ada()])], ..Script::default() };
    let github = StandIn::with(script).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_ada(dir.path(), &github, None);

    hub.check_github_logins(1_000_000).await;

    assert_eq!(named(&hub).as_ref().map(GithubUsername::as_str), Some("ada"));
    assert_eq!(asked_by_number(&github), 0);
}
