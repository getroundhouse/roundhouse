use std::sync::Arc;

use protocol::client::ErrorCode;
use protocol::client::tool_servers::ToolServerAdd;
use protocol::client::tool_sign_ins::{NoToolSignIn, ToolSignInBegun, ToolSignInPage, ToolSignInTo};
use protocol::model::ToolServerAuthGiven;
use tokio::net::TcpListener;
use tool_sign_in::{Asks, Gate, Iss, Person, Says, Script, ServerMetadataAt, StandIn};

use super::support::{
    WORKS, access, finish, finished, new, page, servers, signed_in, start, told, until_asked_for_tokens, visited,
};
use crate::hub::tests::{hub_in, hub_in_again};

#[tokio::test]
async fn a_new_server_is_added_once_its_approved_and_every_client_is_told_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(Script::default()).await);
    let mut feed = hub.subscribe_feed();

    let page = page(&hub, new("i", &stand_in)).await;
    let (before, told_before) = (servers(&hub), told(&mut feed));
    let server = hub.tool_sign_in_finish(visited("i", &stand_in, &page)).await.unwrap();

    assert_eq!((before, told_before), (vec![], vec![]), "nothing is added until it's approved");
    assert_eq!((server.id.as_str(), server.name.as_str(), &server.auth), ("t", "Linear", &WORKS));
    assert_eq!(server.url.as_str(), stand_in.server().as_str());
    assert_eq!(servers(&hub), std::slice::from_ref(&server));
    assert_eq!(told(&mut feed), [server]);
    assert!(page.as_str().contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A17867%2Fcallback"), "{}", page.as_str());
    assert!(access(dir.path()).is_some());
}

#[tokio::test]
async fn a_finish_sent_again_is_answered_with_the_server_after_a_restart_too() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(Script::default()).await);

    let page = page(&hub, new("i", &stand_in)).await;
    let again = hub.tool_sign_in_start(new("i", &stand_in)).await.unwrap_err();
    let finishing = visited("i", &stand_in, &page);
    let (server, at_once_finished) =
        tokio::join!(hub.tool_sign_in_finish(finishing.clone()), hub.tool_sign_in_finish(finishing.clone()));
    let server = server.unwrap();
    let finished_again = hub.tool_sign_in_finish(finishing.clone()).await.unwrap();
    drop(hub);
    let hub = hub_in_again(dir.path());
    let after_a_restart = hub.tool_sign_in_finish(finishing).await.unwrap();

    assert_eq!(again.code, ErrorCode::Conflict, "a start of its ID while it waits");
    assert_eq!(at_once_finished.unwrap(), server);
    assert_eq!((finished_again, after_a_restart), (server.clone(), server));
    assert_eq!(stand_in.asked_for("POST /token").len(), 1, "its code, good once, was used once");
}

#[tokio::test]
async fn one_the_person_refuses_is_ended_and_adds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let script = Script { person: Person::Refuses, ..Script::default() };
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(script).await);
    let page = page(&hub, new("i", &stand_in)).await;
    let finishing = visited("i", &stand_in, &page);

    let refused = hub.tool_sign_in_finish(finishing.clone()).await.unwrap_err();
    let again = hub.tool_sign_in_finish(finishing).await.unwrap_err();

    assert_eq!((refused.code, refused.message.as_str()), (ErrorCode::InvalidRequest, "the sign-in was refused"));
    assert_eq!(again.code, ErrorCode::NotFound, "so another is started");
    assert_eq!(servers(&hub), []);
    assert!(!dir.path().join("tool-servers").exists());
}

/// Such as a page open in the browser that sends what it likes to where the app listens, and an authorization server
/// that isn't the one that was asked, which ends it.
#[tokio::test]
async fn what_isnt_of_the_sign_in_leaves_it_waiting_and_what_isnt_from_its_authorization_server_ends_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(Script::default()).await);
    let of_i = page(&hub, new("i", &stand_in)).await;
    let again = |id| start(id, ToolSignInTo::Again { id: "t".into() });

    let not_its = hub.tool_sign_in_finish(finish("i", "code=made-up&state=made-up")).await.unwrap_err();
    let its = hub.tool_sign_in_finish(visited("i", &stand_in, &of_i)).await;
    let of_another = page(&hub, again("another")).await;
    stand_in.change(|script| script.iss = Iss::Another);
    let from_another = hub.tool_sign_in_finish(visited("another", &stand_in, &of_another)).await.unwrap_err();
    stand_in.change(|script| script.iss = Iss::Said);
    let ended = hub.tool_sign_in_finish(visited("another", &stand_in, &of_another)).await.unwrap_err();

    assert_eq!(not_its.code, ErrorCode::InvalidRequest);
    assert_eq!(its.unwrap().auth, WORKS);
    assert_eq!((from_another.code, ended.code), (ErrorCode::InvalidRequest, ErrorCode::NotFound));
}

#[tokio::test]
async fn a_server_with_no_sign_in_or_no_way_roundhouse_could_says_so_and_waits_for_nothing() {
    type Begins = fn(String) -> ToolSignInBegun;
    let cases: [(Script, Begins); 5] = [
        (Script { asks: Asks::Nothing, server_metadata_at: ServerMetadataAt::Nowhere, ..Script::default() }, |_| {
            ToolSignInBegun::NoSignIn
        }),
        (Script { server_metadata_at: ServerMetadataAt::Nowhere, ..Script::default() }, |_| ToolSignInBegun::NoWay {
            why: NoToolSignIn::NoAuthorizationServer,
        }),
        (Script { client_documents: false, ..Script::default() }, |host| ToolSignInBegun::NoWay {
            why: NoToolSignIn::OnlyRegisteredClients { host },
        }),
        (Script { resource: Says::Another, ..Script::default() }, |_| ToolSignInBegun::NoWay {
            why: NoToolSignIn::OtherServer { host: "other.example".into() },
        }),
        (Script { s256: false, ..Script::default() }, |host| ToolSignInBegun::NoWay {
            why: NoToolSignIn::NoS256 { host },
        }),
    ];
    for (script, expected) in cases {
        let dir = tempfile::tempdir().unwrap();
        let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(script).await);

        let begun = hub.tool_sign_in_start(new("i", &stand_in)).await.unwrap();
        let finished = hub.tool_sign_in_finish(finish("i", "code=made-up&state=made-up")).await.unwrap_err();

        assert_eq!(begun, expected(stand_in.issuer_host()));
        assert_eq!(finished.code, ErrorCode::NotFound);
    }
}

/// Once it answers again, the same start is answered as it's found then.
#[tokio::test]
async fn a_server_that_cant_be_asked_now_may_be_asked_again() {
    let dir = tempfile::tempdir().unwrap();
    let script = Script { asks: Asks::Answers(503), ..Script::default() };
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(script).await);

    let failed = hub.tool_sign_in_start(new("i", &stand_in)).await.unwrap_err();
    stand_in.change(|script| script.asks = Asks::SignIn);
    let begun = hub.tool_sign_in_start(new("i", &stand_in)).await.unwrap();

    assert_eq!(failed.code, ErrorCode::Unavailable);
    assert!(matches!(begun, ToolSignInBegun::Page { .. }), "{begun:?}");
}

#[tokio::test]
async fn a_name_taken_meanwhile_is_refused_and_ends_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(Script::default()).await);
    let page = page(&hub, new("i", &stand_in)).await;
    let (auth, url) = (ToolServerAuthGiven::Neither, "https://mcp.example.com".into());
    let add = ToolServerAdd { id: "another".into(), name: "linear".into(), url, auth };
    let taken = hub.tool_server_add(add).await.unwrap();
    let finishing = visited("i", &stand_in, &page);

    let refused = hub.tool_sign_in_finish(finishing.clone()).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::Conflict);
    assert_eq!(servers(&hub), [taken]);
    assert_eq!(hub.tool_sign_in_finish(finishing).await.unwrap_err().code, ErrorCode::NotFound);
}

#[tokio::test]
async fn one_the_hub_lost_as_it_stopped_isnt_found_so_another_is_started() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(Script::default()).await);
    let page = page(&hub, new("i", &stand_in)).await;
    drop(hub);
    let hub = hub_in_again(dir.path());

    let lost = hub.tool_sign_in_finish(visited("i", &stand_in, &page)).await.unwrap_err();
    let another = finished(&hub, &stand_in, new("another", &stand_in)).await;

    assert_eq!(lost.code, ErrorCode::NotFound);
    assert_eq!(another.unwrap().auth, WORKS);
}

/// As its server is asked how to sign in to it, before there's a page.
#[tokio::test]
async fn one_cancelled_as_its_started_has_no_page() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let (asked, its) = (start("i", ToolSignInTo::New { id: "t".into(), name: "Linear".into(), url }), Arc::clone(&hub));
    let starting = tokio::spawn(async move { its.tool_sign_in_start(asked).await });
    let (connection, _) = listener.accept().await.unwrap();

    hub.tool_sign_in_cancel(&"i".into());
    drop(connection);
    drop(listener);

    assert_eq!(starting.await.unwrap().unwrap_err().code, ErrorCode::Conflict);
}

/// As its code is exchanged for tokens, which come once it was cancelled.
#[tokio::test]
async fn one_cancelled_as_its_finished_adds_nothing_and_one_cancelled_once_its_finished_stays() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::closed();
    let script = Script { answers_after: Some(gate.clone()), ..Script::default() };
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(script).await);
    let page = page(&hub, new("i", &stand_in)).await;
    let (finishing, its) = (visited("i", &stand_in, &page), Arc::clone(&hub));
    let finishing = tokio::spawn(async move { its.tool_sign_in_finish(finishing).await });
    until_asked_for_tokens(&stand_in, 1).await;

    hub.tool_sign_in_cancel(&"i".into());
    hub.tool_sign_in_cancel(&"i".into());
    gate.open();
    let cancelled = finishing.await.unwrap().unwrap_err();
    let nothing = servers(&hub);
    let server = signed_in(&hub, &stand_in).await;
    hub.tool_sign_in_cancel(&"i".into());

    assert_eq!((cancelled.code, nothing), (ErrorCode::NotFound, vec![]));
    assert_eq!(servers(&hub), [server]);
}

/// Asking to be allowed what it was asked before as well, and, at another authorization server, what it says alone.
#[tokio::test]
async fn a_server_signed_in_to_again_asks_for_what_it_had_too_unless_its_authorization_server_is_another() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(Script::default()).await);
    let first = signed_in(&hub, &stand_in).await;
    let before = access(dir.path());
    let asks =
        |page: &ToolSignInPage| page.as_str().split("&scope=").nth(1).unwrap().split('&').next().unwrap().to_string();
    let to = |id: &str| start(id, ToolSignInTo::Again { id: "t".into() });

    stand_in.change(|script| script.challenge_scope = Some("more"));
    let page_again = page(&hub, to("again")).await;
    let again = hub.tool_sign_in_finish(visited("again", &stand_in, &page_again)).await.unwrap();
    stand_in.change(|script| script.issuer_path = "/another");
    let of_another = page(&hub, to("another")).await;

    assert_eq!(asks(&page_again), "more+read+write");
    assert_eq!(asks(&of_another), "more");
    assert_eq!(again, first, "its record says no more than it did");
    assert_ne!(access(dir.path()), before);
}
