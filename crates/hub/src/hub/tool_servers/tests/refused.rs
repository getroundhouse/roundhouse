//! What a worker is answered that asks for another access token than the one its server refused.

use std::path::Path;
use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode};
use protocol::worker::{AskReply, ProxiedServer, RenewToolServer, ServerSignIn, TokenNumber, WorkerAsk};
use tool_sign_in::{Gate, Refreshing, Script, StandIn};

use super::super::renewing::Joined;
use super::support::{NEEDS_SIGN_IN, access, finished, servers, signed_in, start, told, until_asked_for_tokens};
use crate::hub::Hub;
use crate::hub::tests::hub_in;
use crate::worker_link::WorkerLink;

const FIRST: TokenNumber = TokenNumber::FIRST;

/// A hub, and a stand-in answering as `script` says, which its server `t` is signed in to at, with its first token.
async fn hub_signed_in(dir: &Path, script: Script) -> (Arc<Hub>, StandIn) {
    let (hub, stand_in) = (hub_in(dir), StandIn::with(script).await);
    signed_in(&hub, &stand_in).await;
    (hub, stand_in)
}

/// What a worker is answered that asks as `refused` was refused.
async fn asked(hub: &Arc<Hub>, refused: TokenNumber) -> Result<ProxiedServer, ApiError> {
    let ask = WorkerAsk::RenewToolServer(RenewToolServer { server: "t".into(), refused });
    let answer = hub.answer_ask(&"w".into(), &WorkerLink::unconnected(), "s".into(), ask).await?;
    let AskReply::ToolServer(server) = answer else { panic!("{answer:?}") };
    Ok(server)
}

/// Which token a worker was answered with, and all its header says.
fn given(server: &ProxiedServer) -> (Option<ServerSignIn>, Option<String>) {
    (server.sign_in, server.header.as_ref().map(|header| header.value.as_str().to_string()))
}

/// `number` is the one its server has now, at which it's in the hub's file.
fn now(dir: &Path, number: TokenNumber) -> (Option<ServerSignIn>, Option<String>) {
    (Some(ServerSignIn::Works { token: number }), access(dir).map(|token| format!("Bearer {token}")))
}

/// As when the worker asks the next hub what the last didn't answer.
#[tokio::test]
async fn a_worker_is_answered_with_another_token_and_one_that_asks_again_with_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path(), Script::default()).await;
    let before = access(dir.path());

    let answered = asked(&hub, FIRST).await.unwrap();
    let again = asked(&hub, FIRST).await.unwrap();

    assert_ne!(access(dir.path()), before);
    assert_eq!(given(&answered), now(dir.path(), FIRST.next()));
    assert_eq!(again, answered);
    assert_eq!(hub.tool_servers.sent().borrow().set().servers, [answered], "every worker is sent it too");
    assert_eq!(stand_in.asked_for("POST /token").len(), 2, "a code, and one renewal");
}

/// The authorization server holds its answer until both have asked, which a check does besides.
#[tokio::test]
async fn workers_that_ask_at_once_and_a_check_have_one_renewal_between_them() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path(), Script::default()).await;
    let gate = Gate::closed();
    stand_in.change(|script| script.answers_after = Some(gate.clone()));
    let ask = || {
        let hub = Arc::clone(&hub);
        tokio::spawn(async move { asked(&hub, FIRST).await.unwrap() })
    };

    let first = ask();
    until_asked_for_tokens(&stand_in, 2).await;
    let second = ask();
    let due_at_ms = crate::store::now_ms() + 3_500_000;
    hub.check_tool_sign_ins(due_at_ms).await;
    gate.open();
    let (first, second) = (first.await.unwrap(), second.await.unwrap());

    assert_eq!(given(&first), now(dir.path(), FIRST.next()));
    assert_eq!(second, first);
    assert_eq!(stand_in.asked_for("POST /token").len(), 2);
}

/// So an agent that goes on calling a server that refuses every token doesn't have the authorization server asked for
/// one at each call.
#[tokio::test]
async fn a_token_refused_moments_after_it_was_given_is_answered_with_itself_until_the_person_signs_in_again() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = hub_signed_in(dir.path(), Script::default()).await;
    let second = asked(&hub, FIRST).await.unwrap();

    let refused = asked(&hub, FIRST.next()).await.unwrap();
    let renewals = stand_in.asked_for("POST /token").len();
    let to = protocol::client::tool_sign_ins::ToolSignInTo::Again { id: "t".into() };
    finished(&hub, &stand_in, start("again", to)).await.unwrap();
    let signed_in_again = asked(&hub, FIRST.next().next()).await.unwrap();

    assert_eq!(refused, second);
    assert_eq!(renewals, 2, "a code, and one renewal");
    assert_eq!(given(&signed_in_again), now(dir.path(), FIRST.next().next().next()));
}

#[tokio::test]
async fn a_renewal_that_fails_for_now_is_answered_with_the_token_there_is_and_isnt_made_at_each_ask() {
    let dir = tempfile::tempdir().unwrap();
    let script = Script { refreshing: Refreshing::Answers(503), ..Script::default() };
    let (hub, stand_in) = hub_signed_in(dir.path(), script).await;

    let answered = asked(&hub, FIRST).await.unwrap();
    let again = asked(&hub, FIRST).await.unwrap();

    assert_eq!(given(&answered), now(dir.path(), FIRST));
    assert_eq!(again, answered);
    assert_eq!(stand_in.asked_for("POST /token").len(), 2);
    let due = || hub.tool_servers.refused(&"t".into(), FIRST).unwrap().due.remove(0);
    let joins = |after_ms| hub.tool_sign_ins.renewing.join(&due(), crate::store::now_ms() + after_ms);
    assert!(matches!(joins(50_000), Joined::Nothing));
    assert!(matches!(joins(61_000), Joined::Begun(..)), "and it's made again once a while has passed");
}

#[tokio::test]
async fn a_renewal_refused_for_good_is_answered_as_needing_signing_in_to_and_every_client_is_told() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _stand_in) =
        hub_signed_in(dir.path(), Script { refreshing: Refreshing::Refused("invalid_grant"), ..Script::default() })
            .await;
    let mut feed = hub.subscribe_feed();

    let answered = asked(&hub, FIRST).await.unwrap();

    assert_eq!(given(&answered), (Some(ServerSignIn::NeedsSignIn), None));
    let told: Vec<_> = told(&mut feed).into_iter().map(|server| server.auth).collect();
    assert_eq!(told, [NEEDS_SIGN_IN]);
    assert_eq!(servers(&hub)[0].auth, NEEDS_SIGN_IN);
    assert_eq!(given(&asked(&hub, FIRST).await.unwrap()), (Some(ServerSignIn::NeedsSignIn), None), "asked twice");
}

#[tokio::test]
async fn a_server_thats_gone_isnt_found() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _stand_in) = hub_signed_in(dir.path(), Script::default()).await;
    hub.tool_server_remove("t".into()).await.unwrap();

    let refused = asked(&hub, FIRST).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::NotFound);
}
