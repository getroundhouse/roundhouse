//! What a worker is answered whose server wants more access, and signing in to it again. What's kept, and how often it's
//! asked for, is tested in `crate::tool_servers`.

use std::sync::Arc;

use protocol::client::tool_sign_ins::ToolSignInTo;
use protocol::model::{ToolScopes, ToolServerAuth, ToolSignInStatus};
use protocol::worker::{AskForToolAccess, AskReply, TokenNumber, ToolAccess, WorkerAsk};
use tool_sign_in::{SaysAllowed, Script, StandIn};

use super::support::{WORKS, kept, page, servers, signed_in, start, told, visited};
use crate::hub::Hub;
use crate::hub::tests::hub_in;
use crate::worker_link::WorkerLink;

const NEEDS_MORE_ACCESS: ToolServerAuth = ToolServerAuth::SignIn { status: ToolSignInStatus::NeedsMoreAccess };

async fn asked(hub: &Arc<Hub>, refused: TokenNumber) -> ToolAccess {
    let wants = ToolScopes::of(["admin", "write"]);
    let ask = WorkerAsk::AskForToolAccess(AskForToolAccess { server: "t".into(), refused, wants });
    let answer = hub.answer_ask(&"w".into(), &WorkerLink::unconnected(), "s".into(), ask).await.unwrap();
    let AskReply::ToolAccess(access) = answer else { panic!("{answer:?}") };
    access
}

#[tokio::test]
async fn every_client_is_told_once_that_a_server_wants_more_access_and_signing_in_again_asks_for_all_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(Script::default()).await);
    signed_in(&hub, &stand_in).await;
    let mut feed = hub.subscribe_feed();

    let answered = (asked(&hub, TokenNumber::FIRST).await, asked(&hub, TokenNumber::FIRST).await);
    let told: Vec<_> = told(&mut feed).into_iter().map(|server| server.auth).collect();
    let page = page(&hub, start("again", ToolSignInTo::Again { id: "t".into() })).await;
    let again = hub.tool_sign_in_finish(visited("again", &stand_in, &page)).await.unwrap();

    assert_eq!(answered, (ToolAccess::SignInAgain, ToolAccess::SignInAgain));
    assert_eq!(told, [NEEDS_MORE_ACCESS]);
    assert!(page.as_str().contains("&scope=read+write+admin"), "{}", page.as_str());
    assert_eq!((&again.auth, servers(&hub)), (&WORKS, vec![again.clone()]));
}

/// Its first request names no scope, so its authorization server grants its own default, and what asks for a refresh
/// token with it, which is asked for only where the service lists it.
#[tokio::test]
async fn signing_in_again_asks_for_what_was_granted_by_default_too() {
    for (authorization_scopes, asked_for) in
        [(None, "read+write+admin"), (Some(vec!["offline_access"]), "read+write+admin+offline_access")]
    {
        let dir = tempfile::tempdir().unwrap();
        let (says_allowed, allows) = (SaysAllowed::WhatItAllows, Some("offline_access read write"));
        let script = Script { challenge_scope: None, allows, says_allowed, authorization_scopes, ..Script::default() };
        let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(script).await);
        signed_in(&hub, &stand_in).await;
        let first = kept(dir.path());

        asked(&hub, TokenNumber::FIRST).await;
        let page = page(&hub, start("again", ToolSignInTo::Again { id: "t".into() })).await;

        assert_eq!((&first["renewable"]["scopes"], &first["granted"]), (&"".into(), &"read write".into()));
        let scope = page.as_str().split("&scope=").nth(1).unwrap();
        assert_eq!(scope.split('&').next(), Some(asked_for));
    }
}
