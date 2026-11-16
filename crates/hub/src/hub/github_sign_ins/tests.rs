//! Signing in as a client asks, against a hub that never starts a worker and a stand-in for GitHub, on tokio's clock as
//! it runs, since a request to the stand-in races a paused clock, as `crate::github`'s device flow tests say. So the
//! stand-in's codes say a second between asks, the least the hub keeps to, and no test waits for more than one. How
//! often GitHub is asked, and what its answers come to, is tested in `crate::github`, on the paused clock where it
//! matters, and what approving keeps, and what it renews, in `crate::github_logins`.

use std::time::Duration;

use protocol::client::HubNotice;
use protocol::model::{GithubLoginKind, GithubSignInId, GithubSignInStatus, RecordKey};
use tokio::sync::broadcast;

use super::*;
use crate::github::{Answer, Script, StandIn};
use crate::hub::Hub;
use crate::hub::tests::hub_calling_github;

fn start(id: &str) -> GithubSignInStart {
    GithubSignInStart { id: id.into() }
}

fn pending() -> Answer {
    Answer::device_flow_error("authorization_pending")
}

/// A code, `device`, with a second between asks after its token.
fn code(device: &str) -> Answer {
    Answer::code(device, 900, 1)
}

/// A stand-in that gives each sign-in the code `device`, and answers its asks after the token with `polls`, the last
/// again and again, and `GET /user` with `a-token` as ada's.
async fn github(device: &'static str, polls: Vec<Answer>) -> StandIn {
    StandIn::with(Script {
        users: vec![("a-token", Answer::user(7, "ada", None))],
        codes: vec![code(device)],
        polls: vec![(device, polls)],
        ..Script::default()
    })
    .await
}

/// What `waited` comes to, failing rather than hanging when it never comes.
async fn soon<T>(what: &str, waited: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(10), waited).await.unwrap_or_else(|_| panic!("{what} never came"))
}

/// The next notice of a sign-in, or its removal, passing over the rest.
async fn next_sign_in(feed: &mut broadcast::Receiver<HubNotice>) -> HubNotice {
    soon("a notice of a sign-in", async {
        loop {
            if let notice @ (HubNotice::GithubSignIn(_) | HubNotice::Removed { .. }) = feed.recv().await.unwrap() {
                return notice;
            }
        }
    })
    .await
}

/// The next notice of a sign-in that has ended, passing over the rest.
async fn next_ended(feed: &mut broadcast::Receiver<HubNotice>) -> protocol::model::GithubSignIn {
    loop {
        if let HubNotice::GithubSignIn(sign_in) = next_sign_in(feed).await
            && !sign_in.status.is_waiting()
        {
            return sign_in;
        }
    }
}

/// Once no sign-in's job is running.
async fn until_no_job_runs(hub: &Hub) {
    soon("the end of every sign-in's job", async {
        while !hub.sign_ins.busy().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
}

#[tokio::test]
async fn a_sign_in_waits_with_its_code_and_once_approved_every_client_hears_of_its_account_first_and_then_of_it() {
    let github = github("a-device-code", vec![Answer::granted("a-token", "a-refresh-token")]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let mut feed = hub.subscribe_feed();

    let started = hub.github_sign_in_start(start("s")).await.unwrap();
    let mut heard = Vec::new();
    while !matches!(heard.last(), Some(HubNotice::GithubSignIn(sign_in)) if !sign_in.status.is_waiting()) {
        heard.push(soon("a notice", feed.recv()).await.unwrap());
    }
    let again = hub.github_sign_in_start(start("s")).await.unwrap();

    let GithubSignInStatus::Waiting { code, address, .. } = &started.status else { panic!("{started:?}") };
    assert_eq!((code.as_str(), address.as_str()), ("WDJB-MJHT", "https://github.com/login/device"));
    let [
        HubNotice::GithubSignIn(waiting),
        HubNotice::GithubAccount(account),
        HubNotice::HubSettings(settings),
        HubNotice::GithubSignIn(approved),
    ] = heard.as_slice()
    else {
        panic!("{heard:?}")
    };
    assert_eq!(waiting, &started);
    assert_eq!(account.held_login.map(|held| held.how), Some(GithubLoginKind::SignIn));
    assert_eq!(settings.github.as_ref().unwrap().account, Some(account.id.clone()), "the first is the default");
    assert_eq!(approved.status, GithubSignInStatus::Approved { account: account.id.clone() });
    assert_eq!(&again, approved, "the same start again is the same sign-in");
    assert_eq!(github.asked().len(), 1 + 1 + 1, "the code, an ask after the token, and whose it is, once");
    until_no_job_runs(&hub).await;
}

/// Cancelled while its job waits to ask GitHub after it.
#[tokio::test]
async fn cancelling_a_sign_in_ends_its_job_tells_every_client_and_cancelling_again_changes_nothing() {
    let github = github("a-device-code", vec![pending()]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    hub.github_sign_in_start(start("s")).await.unwrap();
    let mut feed = hub.subscribe_feed();

    let cancelled = hub.github_sign_in_cancel("s".into()).await.unwrap();
    let told = next_sign_in(&mut feed).await;
    until_no_job_runs(&hub).await;
    let again = hub.github_sign_in_cancel("s".into()).await.unwrap();
    let nobodys = hub.github_sign_in_cancel("nobody's".into()).await;

    assert_eq!(cancelled.status, GithubSignInStatus::Cancelled);
    assert_eq!(told, HubNotice::GithubSignIn(cancelled.clone()));
    assert_eq!(again, cancelled);
    assert!(feed.try_recv().is_err(), "cancelling again told no one");
    assert_eq!(nobodys.unwrap_err().code, ErrorCode::NotFound);
}

#[tokio::test]
async fn starting_a_sign_in_removes_those_that_ended_and_tells_every_client_they_have_gone() {
    let github = github("a-device-code", vec![Answer::device_flow_error("access_denied")]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let mut feed = hub.subscribe_feed();
    hub.github_sign_in_start(start("refused")).await.unwrap();
    let refused = next_ended(&mut feed).await;
    until_no_job_runs(&hub).await;

    let another = hub.github_sign_in_start(start("another")).await.unwrap();

    assert_eq!(refused.status, GithubSignInStatus::Refused);
    let HubNotice::Removed { record, rev } = next_sign_in(&mut feed).await else { panic!() };
    assert_eq!(record, RecordKey::GithubSignIn { id: "refused".into() });
    assert!(rev > refused.rev, "{rev} isn't past {}", refused.rev);
    assert_eq!(next_sign_in(&mut feed).await, HubNotice::GithubSignIn(another.clone()));
    let listed: Vec<GithubSignInId> =
        hub.records_list().await.unwrap().github_sign_ins.into_iter().map(|sign_in| sign_in.id).collect();
    assert_eq!(listed, ["another".into()]);
    hub.stop_jobs();
}

/// GitHub failing to give a code, and a fifth sign-in while four wait, which isn't asked a code for.
#[tokio::test]
async fn a_sign_in_github_gives_no_code_for_or_past_as_many_as_may_wait_is_refused_and_keeps_nothing() {
    let codes = vec![Answer::status(503, Vec::new()), code("a-device-code")];
    let github =
        StandIn::with(Script { codes, polls: vec![("a-device-code", vec![pending()])], ..Script::default() }).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());

    let unavailable = hub.github_sign_in_start(start("unavailable")).await;
    for id in ["1", "2", "3", "4"] {
        hub.github_sign_in_start(start(id)).await.unwrap();
    }
    let asked_for_codes = || github.asked().iter().filter(|asked| asked.starts_with("post /login/device/code")).count();
    let before = asked_for_codes();
    let fifth = hub.github_sign_in_start(start("5")).await;

    assert_eq!(unavailable.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(fifth.unwrap_err().code, ErrorCode::Conflict);
    assert_eq!(asked_for_codes(), before, "GitHub wasn't asked for the fifth's code");
    let listed = hub.records_list().await.unwrap().github_sign_ins;
    assert_eq!(listed.iter().map(|sign_in| sign_in.id.as_str()).collect::<Vec<_>>(), ["1", "2", "3", "4"]);
    hub.stop_jobs();
}

/// One sign-in's ask after its token never answered while another's is approved.
#[tokio::test]
async fn a_github_that_never_answers_one_sign_in_holds_up_only_that_one() {
    let github = StandIn::with(Script {
        users: vec![("a-token", Answer::user(7, "ada", None))],
        codes: vec![code("stuck"), code("answered")],
        polls: vec![("stuck", vec![Answer::Never]), ("answered", vec![Answer::granted("a-token", "a-refresh-token")])],
        ..Script::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let mut feed = hub.subscribe_feed();

    hub.github_sign_in_start(start("stuck")).await.unwrap();
    hub.github_sign_in_start(start("answered")).await.unwrap();
    let ended = next_ended(&mut feed).await;
    soon("the stuck one's ask", github.until_asked(2 + 2 + 1)).await;

    assert_eq!(ended.id.as_str(), "answered");
    assert!(matches!(ended.status, GithubSignInStatus::Approved { .. }), "{ended:?}");
    soon("the answered one's job ending", async {
        while hub.sign_ins.busy() != [GithubSignInId::from("stuck")] {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    hub.stop_jobs();
}

/// The hub stopping ends every sign-in's job where it is, and starts none after. The next hub ends what they left,
/// as `crate::run` tests.
#[tokio::test]
async fn the_hub_stopping_ends_every_sign_ins_job_and_starts_none_after() {
    let github = StandIn::with(Script { codes: vec![code("a-device-code")], ..Script::default() }).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    hub.github_sign_in_start(start("s")).await.unwrap();

    hub.stop_jobs();
    let after = hub.github_sign_in_start(start("after")).await;

    assert_eq!(hub.sign_ins.busy(), Vec::<GithubSignInId>::new());
    assert_eq!(after.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(github.asked().len(), 1, "only the first's code was asked for");
}
