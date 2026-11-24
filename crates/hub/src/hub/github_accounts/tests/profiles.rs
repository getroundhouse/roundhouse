//! Asking GitHub again who an account is, as the hub's check does, against a hub that never starts a worker and a
//! stand-in for GitHub, on tokio's clock as it runs, as `renewals` says. Which accounts are due, and what each of
//! GitHub's answers keeps, is tested in `crate::github_logins`.

use std::sync::Arc;
use std::time::Duration;

use protocol::model::{GithubToken, GithubUser, GithubUserId, Observation, Observed};
use protocol::worker::{GitIdentity, GithubCredential};

use super::*;
use crate::github_logins::{Connect, DefaultCredential};
use crate::hub::Hub;
use crate::hub::tests::hub_calling_github;
use crate::store::now_ms;
use github::GithubProfile;
use github::{Answer, Gate, StandIn, TokenOwner};

/// A hub calling `github`, whose default is ada, connected at 1000 by her token, `a-token`, with nothing kept of who
/// she is on GitHub, as on the first start after the hub stopped keeping it in its database, so the first check asks.
fn hub_with_ada(dir: &std::path::Path, github: &StandIn) -> Arc<Hub> {
    let hub = hub_calling_github(dir, github.api());
    let connect = Connect::new(GithubAccountConnect { token: GithubToken::new("a-token"), id: None }, 1000).unwrap();
    let user = GithubUser { id: GithubUserId::new(7), name: "ada".into() };
    let ada = TokenOwner { user, profile: GithubProfile::default(), ends_at_ms: None };
    hub.github_logins.connect(&connect, &ada).unwrap();
    hub
}

/// Ada has named herself on GitHub since, shows an email, and has renamed herself there. Until the check, the hub
/// doesn't know what GitHub calls her, so workers are sent a block, and pull requests aren't read as her. The check
/// asks GitHub, and clients hear of her username, a client of version 4 on her account's record, and workers are sent
/// her name.
#[tokio::test]
async fn a_check_asks_github_who_an_account_is_and_clients_hear_of_its_new_username_and_workers_of_its_name() {
    let renamed = Answer::user_with_profile(7, "ada-lovelace", Some("Ada Lovelace"), Some("ada@example.com"));
    let github = StandIn::start(vec![("a-token", renamed)]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_ada(dir.path(), &github);
    let mut feed = hub.subscribe_feed();
    let unnamed = hub.git_credential().borrow().clone();
    let read_as_before = hub.reads_as().map_err(|e| e.code);

    hub.check_github_logins(now_ms()).await;
    hub.check_github_logins(now_ms()).await;

    assert!(matches!(unnamed, DefaultCredential::Unnamed { .. }), "{unnamed:?}");
    assert_eq!(read_as_before, Err(ErrorCode::Worker), "no pull request is read as an account not named");
    let told: Vec<HubNotice> =
        std::iter::from_fn(|| feed.try_recv().ok().map(std::sync::Arc::unwrap_or_clone)).collect();
    let [HubNotice::Observation(Observation::GithubUsername { username, .. }), HubNotice::GithubAccount(_)] =
        told.as_slice()
    else {
        panic!("{told:?}")
    };
    assert!(matches!(username, Observed::AsOf { value, .. } if value.as_str() == "ada-lovelace"), "{username:?}");
    let GithubCredential::Account(credential) = hub.git_credential().borrow().for_workers() else { panic!() };
    let identity = GitIdentity { name: "Ada Lovelace".to_string(), email: "ada@example.com".to_string() };
    assert_eq!((credential.username.as_str(), credential.identity), ("ada-lovelace", Some(identity)));
    assert_eq!(github.asked().len(), 1, "the check waited for GitHub's answer, and the next didn't ask again");
}

/// GitHub holding back who ada is, as the first check asks, until after the stop. A stop that waited would wait for the
/// call's 15 s, so each wait here fails after 5 s rather than wait on.
#[tokio::test]
async fn a_stop_while_the_check_asks_github_who_an_account_is_waits_for_no_answer() {
    let gate = Gate::closed();
    let held_back = Answer::After(gate.clone(), Box::new(Answer::user(7, "ada", None)));
    let github = StandIn::start(vec![("a-token", held_back)]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_ada(dir.path(), &github);
    let watching = tokio::spawn(Arc::clone(&hub).watch_github_logins());
    let asking = tokio::time::timeout(Duration::from_secs(5), github.until_asked(1)).await;

    let stopping = tokio::time::timeout(Duration::from_secs(5), hub.stop_checking_github_logins(watching)).await;

    assert!(asking.is_ok(), "the check didn't ask GitHub who ada is");
    assert!(stopping.is_ok(), "the stop waited for GitHub");
    gate.open();
}
