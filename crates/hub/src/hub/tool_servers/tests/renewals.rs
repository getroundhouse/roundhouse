//! Each check here is at a time made up, by the wall clock, while what an answer is kept as is by the clock as it runs.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use protocol::client::HubNotice;
use protocol::model::RecordKey;
use tool_sign_in::{CLIENT_DOCUMENT, Gate, Refreshing, Renewal, Script, StandIn, ToolSignIn};

use super::support::{
    NEEDS_SIGN_IN, WORKS, access, finished, kept, servers, signed_in, start, told, until_asked_for_tokens,
};
use crate::hub::Hub;
use crate::hub::tests::{hub_in, hub_in_again};
use crate::store::now_ms;

/// The stand-in's tokens last an hour, so one is due by now.
fn due_at_ms() -> i64 {
    after(Duration::from_mins(58))
}

fn after(wait: Duration) -> i64 {
    now_ms() + i64::try_from(wait.as_millis()).unwrap()
}

/// Checks at `now_ms`, and waits for each renewal under way.
async fn checked(hub: &Arc<Hub>, now_ms: i64) {
    hub.check_tool_sign_ins(now_ms).await;
    hub.tool_sign_ins.renewing.ended().await;
}

/// A hub, and a stand-in answering as `script` says, which its server `t` is signed in to at.
async fn hub_signed_in(dir: &Path, script: Script) -> (Arc<Hub>, StandIn) {
    let (hub, stand_in) = (hub_in(dir), StandIn::with(script).await);
    signed_in(&hub, &stand_in).await;
    (hub, stand_in)
}

#[tokio::test]
async fn a_check_renews_a_sign_in_ahead_of_its_tokens_end_and_not_before_and_no_client_is_told() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path(), Script::default()).await;
    let (first, before) = (access(dir.path()), servers(&hub));
    let mut feed = hub.subscribe_feed();

    checked(&hub, after(Duration::from_mins(50))).await;
    let early = access(dir.path());
    checked(&hub, due_at_ms()).await;
    let renewed = access(dir.path());

    assert_eq!(early, first);
    assert!(renewed.is_some() && renewed != first);
    assert_eq!(servers(&hub), before);
    assert_eq!(told(&mut feed), []);
    let asked = stand_in.asked_for("POST /token");
    assert_eq!(asked.len(), 2, "a code, and one renewal");
    assert_eq!(asked[1].form("grant_type").as_deref(), Some("refresh_token"));
    assert!(kept(dir.path())["tokens"]["ends_at_ms"].as_i64().unwrap() > after(Duration::from_mins(59)));
}

/// As one that stopped with it due does.
#[tokio::test]
async fn a_hub_that_starts_renews_whats_due() {
    let dir = tempfile::tempdir().unwrap();
    let script = Script { lasts: Some(60), ..Script::default() };
    let (hub, stand_in) = hub_signed_in(dir.path(), script).await;
    let first = access(dir.path());
    drop(hub);
    let hub = hub_in_again(dir.path());

    let watching = tokio::spawn(Arc::clone(&hub).watch_tool_sign_ins());
    until_asked_for_tokens(&stand_in, 2).await;
    hub.tool_sign_ins.renewing.ended().await;
    watching.abort();

    assert_ne!(access(dir.path()), first);
}

#[tokio::test]
async fn one_refused_for_good_needs_signing_in_to_as_every_client_is_told_and_signing_in_again_mends_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path(), Script::default()).await;
    let mut feed = hub.subscribe_feed();
    stand_in.change(|script| script.refreshing = Refreshing::Refused("invalid_grant"));

    checked(&hub, due_at_ms()).await;
    let gone = told(&mut feed);
    let tokens = kept(dir.path())["tokens"].clone();
    checked(&hub, due_at_ms()).await;
    let asked = stand_in.asked_for("POST /token").len();
    let to = protocol::client::tool_sign_ins::ToolSignInTo::Again { id: "t".into() };
    let again = finished(&hub, &stand_in, start("again", to)).await.unwrap();

    assert_eq!(gone.iter().map(|server| &server.auth).collect::<Vec<_>>(), [&NEEDS_SIGN_IN]);
    assert!(tokens.is_null(), "{tokens}");
    assert_eq!(asked, 2, "and it isn't asked again");
    assert_eq!(again.auth, WORKS);
    assert_eq!(told(&mut feed), [again]);
}

/// Each token lasts a minute, so it's due at every check. `invalid_client` of the client document is what a service says
/// while it can't fetch the document, as after a bad deploy of the site, so the refresh token is kept for when it can.
#[tokio::test]
async fn one_that_fails_for_now_is_left_as_it_is_and_tried_again_after_waits_that_grow() {
    for refreshing in [Refreshing::Answers(503), Refreshing::Refused("invalid_client")] {
        let dir = tempfile::tempdir().unwrap();
        let script = Script { lasts: Some(60), refreshing, ..Script::default() };
        let (hub, stand_in) = hub_signed_in(dir.path(), script).await;
        let (before, records) = (kept(dir.path()), servers(&hub));
        let seconds = |seconds| after(Duration::from_secs(seconds));
        let asked = || stand_in.asked_for("POST /token").len() - 1;

        let mut tried = Vec::new();
        for check in [seconds(0), seconds(30), seconds(65), seconds(100), seconds(125)] {
            checked(&hub, check).await;
            tried.push(asked());
        }
        let (after_failing, records_after) = (kept(dir.path()), servers(&hub));
        stand_in.change(|script| script.refreshing = Refreshing::Works);
        checked(&hub, seconds(300)).await;

        assert_eq!(tried, [1, 1, 2, 2, 3], "{refreshing:?}: a minute after the first, and two after the second");
        assert_eq!((after_failing, records_after), (before.clone(), records), "{refreshing:?}");
        assert_ne!(kept(dir.path()), before, "{refreshing:?}");
        assert!(kept(dir.path())["tokens"]["refresh"].is_string(), "{refreshing:?}");
    }
}

/// The authorization server holds its answer, as one that's slow does.
#[tokio::test]
async fn checks_while_a_sign_in_is_being_renewed_renew_it_once() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path(), Script::default()).await;
    let gate = Gate::closed();
    stand_in.change(|script| script.answers_after = Some(gate.clone()));

    hub.check_tool_sign_ins(due_at_ms()).await;
    hub.check_tool_sign_ins(due_at_ms()).await;
    until_asked_for_tokens(&stand_in, 2).await;
    hub.check_tool_sign_ins(due_at_ms()).await;
    gate.open();
    hub.tool_sign_ins.renewing.ended().await;

    assert_eq!(stand_in.asked_for("POST /token").len(), 2);
    assert_eq!(servers(&hub)[0].auth, WORKS);
}

/// As one found due just before another renewal of it ended, which an authorization server may take for a stolen token.
#[tokio::test]
async fn one_renewed_since_it_was_found_due_isnt_renewed_with_the_refresh_token_it_had() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path(), Script::default()).await;
    let found = hub.tool_servers.check_sign_ins(due_at_ms()).unwrap().due.remove(0);

    checked(&hub, due_at_ms()).await;
    hub.renew_tool_sign_in(found);
    hub.tool_sign_ins.renewing.ended().await;

    assert_eq!(stand_in.asked_for("POST /token").len(), 2, "a code, and one renewal");
}

#[tokio::test]
async fn a_server_removed_as_its_sign_in_is_renewed_stays_removed_with_nothing_kept() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path(), Script::default()).await;
    let gate = Gate::closed();
    stand_in.change(|script| script.answers_after = Some(gate.clone()));
    hub.check_tool_sign_ins(due_at_ms()).await;
    until_asked_for_tokens(&stand_in, 2).await;
    let mut feed = hub.subscribe_feed();

    hub.tool_server_remove("t".into()).await.unwrap();
    gate.open();
    hub.tool_sign_ins.renewing.ended().await;

    assert_eq!(servers(&hub), []);
    assert_eq!(std::fs::read_dir(dir.path().join("tool-servers")).unwrap().count(), 0);
    let HubNotice::Removed { record, .. } = Arc::unwrap_or_clone(feed.try_recv().unwrap()) else { panic!() };
    assert_eq!(record, RecordKey::ToolServer { id: "t".into() });
    assert!(feed.try_recv().is_err(), "nothing more is told");
}

/// As a hub that's killed once its authorization server has answered, and before it has kept what it gave: the refresh
/// token the next hub has is one its authorization server has replaced, or, where it replaces none, still good.
#[tokio::test]
async fn one_whose_renewal_was_lost_before_it_was_kept_is_renewed_again_or_needs_signing_in_to() {
    for (rotates, after) in [(true, NEEDS_SIGN_IN), (false, WORKS)] {
        let dir = tempfile::tempdir().unwrap();
        let script = Script { rotates, ..Script::default() };
        let (hub, _stand_in) = hub_signed_in(dir.path(), script).await;
        let due = hub.tool_servers.check_sign_ins(due_at_ms()).unwrap().due.remove(0);
        let lost = ToolSignIn::new(CLIENT_DOCUMENT).renew(&due.renewable, &due.refresh).await.unwrap();
        assert!(matches!(lost, Renewal::Renewed(_)), "{lost:?}");
        drop(hub);
        let hub = hub_in_again(dir.path());
        hub.tidy_tool_servers().await;

        checked(&hub, due_at_ms()).await;

        assert_eq!(servers(&hub)[0].auth, after, "rotates: {rotates}");
        assert_eq!(access(dir.path()).is_some(), after == WORKS);
    }
}

/// As a hub that's killed once it has kept what it was given.
#[tokio::test]
async fn one_whose_renewal_was_kept_as_the_hub_stopped_is_renewed_next_with_what_it_was_given() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _stand_in) = hub_signed_in(dir.path(), Script::default()).await;
    checked(&hub, due_at_ms()).await;
    let second = access(dir.path());
    drop(hub);
    let hub = hub_in_again(dir.path());
    hub.tidy_tool_servers().await;

    checked(&hub, due_at_ms()).await;

    assert!(access(dir.path()).is_some() && access(dir.path()) != second);
    assert_eq!(servers(&hub)[0].auth, WORKS);
}

#[tokio::test]
async fn a_stop_as_a_sign_in_is_renewed_waits_for_its_answer_keeps_it_and_starts_no_other() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path(), Script::default()).await;
    let first = access(dir.path());
    let gate = Gate::closed();
    stand_in.change(|script| script.answers_after = Some(gate.clone()));
    hub.check_tool_sign_ins(due_at_ms()).await;
    until_asked_for_tokens(&stand_in, 2).await;

    let its = Arc::clone(&hub);
    let stopping = tokio::spawn(async move { its.stop_renewing_tool_sign_ins().await });
    gate.open();
    stopping.await.unwrap();
    let kept = access(dir.path());
    checked(&hub, due_at_ms()).await;

    assert!(kept.is_some() && kept != first);
    assert_eq!(access(dir.path()), kept);
    assert_eq!(stand_in.asked_for("POST /token").len(), 2);
}
