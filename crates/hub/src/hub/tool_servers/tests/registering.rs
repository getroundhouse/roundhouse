//! Signing in to a server whose authorization server takes no client document: when the hub registers with it, and when
//! it uses the registration it has. What's sent and what's made of the answer is tested in the `tool-sign-in` crate.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use protocol::client::ErrorCode;
use protocol::client::tool_sign_ins::{NoToolSignIn, ToolSignInBegun, ToolSignInStart, ToolSignInTo, redirect};
use serde_json::json;
use tool_sign_in::{AuthorizationPage, Gate, Person, Refreshing, Registers, Script, StandIn};

use super::support::{NEEDS_SIGN_IN, WORKS, finished, kept, new, page, servers, signed_in, start, visited};
use crate::hub::Hub;
use crate::hub::tests::{hub_in, hub_in_again};
use crate::store::now_ms;

/// Why a server whose authorization server is the one given can't be signed in to.
type Why = fn(String) -> NoToolSignIn;

fn registers(registers: Registers) -> Script {
    Script { client_documents: false, registers, ..Script::default() }
}

/// A hub, and a stand-in that registers any client, which its server `t` is signed in to at.
async fn hub_signed_in(dir: &Path) -> (Arc<Hub>, StandIn) {
    let (hub, stand_in) = (hub_in(dir), StandIn::with(registers(Registers::Anyone)).await);
    signed_in(&hub, &stand_in).await;
    (hub, stand_in)
}

fn again(id: &str) -> ToolSignInStart {
    start(id, ToolSignInTo::Again { id: "t".into() })
}

/// The client that's registered, in the hub's file, if one is.
fn client(dir: &Path) -> Option<String> {
    kept(dir)["registration"]["client_id"].as_str().map(str::to_string)
}

fn times_asked(stand_in: &StandIn) -> usize {
    stand_in.asked_for("POST /register").len()
}

/// Starts signing in again as `id`, and says whether the page would send the person back, which is as far as they get
/// on one that shows an error.
async fn would_be_sent_back(hub: &Hub, stand_in: &StandIn, id: &str) -> bool {
    let page = page(hub, again(id)).await;
    stand_in.sends_back(&AuthorizationPage::new(page.as_str()))
}

/// Checks once the stand-in's tokens, which last an hour, are due, and waits for the renewal.
async fn renew(hub: &Arc<Hub>) {
    hub.check_tool_sign_ins(now_ms() + 3_500_000).await;
    hub.tool_sign_ins.renewing.ended().await;
}

#[tokio::test]
async fn a_registration_is_kept_with_its_sign_in_and_used_to_renew_it_and_to_sign_in_again_after_a_restart_too() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path()).await;
    let first = client(dir.path()).unwrap();

    renew(&hub).await;
    finished(&hub, &stand_in, again("again")).await.unwrap();
    drop(hub);
    let hub = hub_in_again(dir.path());
    finished(&hub, &stand_in, again("more")).await.unwrap();

    assert_eq!(client(dir.path()), Some(first.clone()));
    assert_eq!(times_asked(&stand_in), 1);
    let asked = stand_in.asked_for("POST /token");
    assert_eq!(asked.len(), 4, "three sign-ins, and a renewal");
    assert!(asked.iter().all(|asked| asked.form("client_id").as_ref() == Some(&first)));
    let text = kept(dir.path()).to_string();
    assert!(!text.contains("made-up-secret") && !text.contains("made-up-registration-token"), "{text}");
}

#[tokio::test]
async fn another_issuer_and_another_place_to_be_sent_back_to_are_registered_with_anew() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path()).await;
    let first = client(dir.path());

    let elsewhere = |id| ToolSignInStart { redirect: redirect(4321), ..again(id) };
    page(&hub, elsewhere("unfinished")).await;
    assert_eq!(times_asked(&stand_in), 2);
    page(&hub, again("here")).await;
    assert_eq!(times_asked(&stand_in), 2, "that wasn't used, so nothing has come of it that puts it in doubt");
    hub.tool_sign_in_cancel(&"unfinished".into());
    finished(&hub, &stand_in, elsewhere("elsewhere")).await.unwrap();
    let second = client(dir.path());
    stand_in.change(|script| script.issuer_path = "/another");
    finished(&hub, &stand_in, again("another")).await.unwrap();
    let third = client(dir.path());

    assert!(first != second && second != third, "{first:?}, {second:?}, {third:?}");
    assert_eq!(times_asked(&stand_in), 4);
}

#[tokio::test]
async fn a_sign_in_made_with_the_document_stays_so_and_its_taken_once_its_taken_at_all() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(Script::default()).await);
    signed_in(&hub, &stand_in).await;
    let by_document = kept(dir.path());
    assert!(by_document.get("registration").is_none(), "{by_document}");
    stand_in.change(|script| {
        script.client_documents = false;
        script.registers = Registers::Anyone;
    });

    renew(&hub).await;
    let renewed = kept(dir.path());
    finished(&hub, &stand_in, again("registers")).await.unwrap();
    let registered = kept(dir.path());
    stand_in.change(|script| script.client_documents = true);
    finished(&hub, &stand_in, again("document")).await.unwrap();
    let document = kept(dir.path());

    let client_id = |kept: &serde_json::Value| kept["renewable"]["client_id"].clone();
    assert_ne!(renewed["tokens"], by_document["tokens"]);
    assert_eq!(client_id(&renewed), client_id(&by_document));
    assert_eq!(client_id(&registered), registered["registration"]["client_id"]);
    assert_eq!(client_id(&document), client_id(&by_document));
    assert!(document.get("registration").is_none(), "{document}");
    assert_eq!(times_asked(&stand_in), 1);
}

/// The service shows the person an error on its own page, so nothing comes back to say it has forgotten.
#[tokio::test]
async fn a_registration_its_service_has_forgotten_is_got_past_at_the_next_try() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path()).await;
    let first = client(dir.path());
    stand_in.forget_clients();

    let used_again = would_be_sent_back(&hub, &stand_in, "first").await;
    let kept_meanwhile = client(dir.path());
    hub.tool_sign_in_cancel(&"first".into());
    let server = finished(&hub, &stand_in, again("second")).await.unwrap();
    let second = client(dir.path());
    finished(&hub, &stand_in, again("third")).await.unwrap();

    assert!(!used_again && kept_meanwhile == first, "nothing is changed until one's finished");
    assert_eq!(server.auth, WORKS);
    assert_ne!(second, first);
    assert_eq!((client(dir.path()), times_asked(&stand_in)), (second, 2), "and the new one is used again");
}

/// The next hub has only the file to go by, so it tries the registration once more.
#[tokio::test]
async fn a_forgotten_registration_is_got_past_with_one_more_try_where_the_hub_stopped_between() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path()).await;
    stand_in.forget_clients();

    let before = would_be_sent_back(&hub, &stand_in, "first").await;
    drop(hub);
    let hub = hub_in_again(dir.path());
    let after = would_be_sent_back(&hub, &stand_in, "second").await;
    let third = finished(&hub, &stand_in, again("third")).await;

    assert_eq!((before, after), (false, false));
    assert_eq!(third.unwrap().auth, WORKS);
    assert_eq!(times_asked(&stand_in), 2);
}

/// Having registered in vain, as where nobody finishes that either, isn't a reason to go back to the one it was for.
#[tokio::test]
async fn a_registration_in_doubt_stays_so_until_the_server_is_signed_in_to() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path()).await;
    let first = client(dir.path());

    let mut asked = Vec::new();
    for id in ["first", "second", "third"] {
        let page = page(&hub, again(id)).await;
        asked.push(times_asked(&stand_in));
        if id == "second" {
            // Which says nothing of the one in doubt, since this isn't it.
            stand_in.change(|script| script.person = Person::Refuses);
            hub.tool_sign_in_finish(visited(id, &stand_in, &page)).await.unwrap_err();
            stand_in.change(|script| script.person = Person::Approves);
        }
    }
    let waited = page(&hub, again("first")).await;
    hub.tool_sign_in_finish(visited("first", &stand_in, &waited)).await.unwrap();
    page(&hub, again("fourth")).await;

    assert_eq!(asked, [1, 2, 3]);
    assert_eq!(client(dir.path()), first, "it was good all along");
    assert_eq!(times_asked(&stand_in), 3, "so it's used again");
}

/// After a restart, so that only what's in the file says whether to register.
#[tokio::test]
async fn one_its_authorization_server_says_it_doesnt_know_is_dropped_as_its_renewed_or_finished_with() {
    for (error, asked) in [("invalid_client", 2), ("invalid_grant", 1)] {
        let dir = tempfile::tempdir().unwrap();
        let (hub, stand_in) = hub_signed_in(dir.path()).await;
        stand_in.change(|script| script.refreshing = Refreshing::Refused(error));
        renew(&hub).await;
        assert_eq!(servers(&hub)[0].auth, NEEDS_SIGN_IN);
        drop(hub);
        let hub = hub_in_again(dir.path());

        finished(&hub, &stand_in, again("again")).await.unwrap();

        assert_eq!(times_asked(&stand_in), asked, "{error}");
    }

    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path()).await;
    let first = client(dir.path());
    stand_in.change(|script| script.person = Person::Refuses);
    finished(&hub, &stand_in, again("refused")).await.unwrap_err();
    stand_in.change(|script| script.person = Person::Approves);
    assert!(first.is_some() && client(dir.path()) == first, "one the person refused is kept");
    let known = page(&hub, again("known")).await;
    assert!(known.as_str().contains(first.as_ref().unwrap()) && times_asked(&stand_in) == 1, "and used again");
    drop(hub);
    let hub = hub_in_again(dir.path());
    let (tokens, reused) = (kept(dir.path())["tokens"].clone(), page(&hub, again("again")).await);
    let returned = visited("again", &stand_in, &reused);
    stand_in.forget_clients();
    let unknown = hub.tool_sign_in_finish(returned).await.unwrap_err();
    assert_eq!(unknown.code, ErrorCode::InvalidRequest);
    assert!(unknown.message.contains("invalid_client"), "{}", unknown.message);
    assert_eq!((client(dir.path()), &kept(dir.path())["tokens"]), (None, &tokens), "its sign-in is left as it is");
    drop(hub);
    let hub = hub_in_again(dir.path());
    assert_eq!(finished(&hub, &stand_in, again("more")).await.unwrap().auth, WORKS);
    assert_eq!(times_asked(&stand_in), 2);
}

#[tokio::test]
async fn one_cancelled_as_its_server_is_asked_registers_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::closed();
    let script = Script { metadata_after: Some(gate.clone()), ..registers(Registers::Anyone) };
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(script).await);
    let (asked, its) = (new("i", &stand_in), Arc::clone(&hub));
    let starting = tokio::spawn(async move { its.tool_sign_in_start(asked).await });
    let metadata = || !stand_in.asked_for("GET /metadata").is_empty();
    waiting::task::until(Duration::from_secs(10), "its metadata wasn't asked for", metadata).await;

    hub.tool_sign_in_cancel(&"i".into());
    gate.open();

    assert_eq!(starting.await.unwrap().unwrap_err().code, ErrorCode::Conflict);
    assert_eq!(times_asked(&stand_in), 0);
}

#[tokio::test]
async fn one_that_wont_register_roundhouse_says_why_and_waits_for_nothing_and_one_that_cant_now_may_be_asked_again() {
    let refusal = |error: &str| Registers::Says(400, json!({"error": error, "error_description": "made up"}));
    let no_way: [(Script, Why); 3] = [
        (registers(refusal("invalid_redirect_uri")), |authorization_server| NoToolSignIn::NotRegistered {
            authorization_server,
            error: Some("invalid_redirect_uri".into()),
        }),
        (registers(Registers::Says(403, json!({}))), |authorization_server| NoToolSignIn::NotRegistered {
            authorization_server,
            error: None,
        }),
        (
            Script { client_auth_methods: Some(vec!["client_secret_post"]), ..registers(Registers::Anyone) },
            |authorization_server| NoToolSignIn::OnlyClientsWithSecrets { authorization_server },
        ),
    ];
    for (script, expected) in no_way {
        let dir = tempfile::tempdir().unwrap();
        let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(script).await);

        let begun = hub.tool_sign_in_start(new("i", &stand_in)).await.unwrap();

        assert_eq!(begun, ToolSignInBegun::NoWay { why: expected(stand_in.issuer()) });
        assert!(hub.tool_sign_ins.waiting.get(&"i".into(), now_ms()).is_none());
    }

    let dir = tempfile::tempdir().unwrap();
    let stand_in = StandIn::with(registers(Registers::Says(503, json!({})))).await;
    let hub = hub_in(dir.path());
    let down = hub.tool_sign_in_start(new("i", &stand_in)).await.unwrap_err();
    stand_in.change(|script| script.registers = Registers::Anyone);
    let up = finished(&hub, &stand_in, new("i", &stand_in)).await;
    assert_eq!((down.code, up.unwrap().auth), (ErrorCode::Unavailable, WORKS));
}
