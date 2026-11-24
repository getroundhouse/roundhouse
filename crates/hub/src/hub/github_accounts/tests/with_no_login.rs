//! Asking GitHub with no login who an account is whose own login can't be asked with, as the hub's check does, against
//! a stand-in for GitHub that answers `GET /user/{number}` only when it's asked with no login. Which accounts are due,
//! and how often the hub asks so, is tested in `crate::github_logins`.

use std::sync::Arc;

use protocol::model::{GithubToken, GithubUser, GithubUserId, GithubUsername};

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
    let connect = Connect::new(GithubAccountConnect { token: GithubToken::new("a-token"), id: None }, 1000).unwrap();
    let user = GithubUser { id: GithubUserId::new(7), name: "ada".into() };
    hub.github_logins.connect(&connect, &TokenOwner { user, profile: GithubProfile::default(), ends_at_ms }).unwrap();
    hub
}

/// What the hub calls ada's account's user, if it knows.
fn named(hub: &Hub) -> Option<GithubUsername> {
    let account = hub.store.hub_settings().unwrap().github.and_then(|github| github.account).unwrap();
    hub.observation_value::<crate::hub::GithubUsername>(&account)
}

/// How many times GitHub was asked for user 7 by number.
fn asked_by_number(github: &StandIn) -> usize {
    github.asked().iter().filter(|asked| asked.starts_with("get /user/7 ")).count()
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
