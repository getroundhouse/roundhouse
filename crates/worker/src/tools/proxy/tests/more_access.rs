//! A server that's signed in to, and refuses a call for want of more access than its sign-in has. Each scope is made up
//! by name, so no answer of an agent's may have one.

use http_body_util::{Either, Full};
use hyper::{Response, StatusCode};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::ToolScopes;
use protocol::worker::{AskForToolAccess, AskReply, ServerSignIn, TokenNumber, ToolAccess, WorkerAsk};

use super::signed_in::{CALL, FIRST, answered_by, renewing, with};
use super::{Heard, Reply, StandIn, as_set};

const WANTS: &str =
    r#"Bearer error="insufficient_scope", scope="made-up:read made-up:write", error_description="made-up""#;
const LIST: &str = r#"{"jsonrpc":"2.0","id":"call-8","method":"tools/list"}"#;
const SAID: &str = "Linear (Northwind) wants more access for this than its sign-in has (403). ";
const SIGN_IN_AGAIN: &str = "Sign in to it again from Settings › MCP servers.";
const NO_MORE: &str = "Signing in to it again would ask for no more, so the account signed in with may not have it.";

fn reply(status: StatusCode, challenge: Option<&str>, asked: &Heard) -> Reply {
    let reply = Response::builder().status(status).header("www-authenticate", challenge.unwrap_or("Bearer"));
    reply.body(Either::Left(Full::new(asked.body.clone()))).unwrap()
}

/// Refuses each call of a tool with `challenge`, and answers anything else with its own body.
async fn refusing_calls(challenge: Option<&'static str>) -> StandIn {
    StandIn::answering(move |asked| {
        let calls = asked.body.as_ref() == CALL.as_bytes();
        if calls { reply(StatusCode::FORBIDDEN, challenge, asked) } else { reply(StatusCode::OK, None, asked) }
    })
    .await
}

const SIGNS_IN_AGAIN: AskReply = AskReply::ToolAccess(ToolAccess::SignInAgain);

fn asking(refused: TokenNumber) -> WorkerAsk {
    let wants = ToolScopes::of(["made-up:read", "made-up:write"]);
    WorkerAsk::AskForToolAccess(AskForToolAccess { server: "t-1".into(), refused, wants })
}

#[tokio::test]
async fn a_call_that_wants_more_access_is_told_to_the_hub_and_is_the_only_one_that_fails() {
    let server = refusing_calls(Some(WANTS)).await;
    let set = with(&server, "made-up-first", FIRST);
    let (agent, asks) = answered_by(set, |_| Ok(SIGNS_IN_AGAIN)).await;

    let (id, said) = agent.post(CALL).await.error();
    let listed = agent.post(LIST).await;

    assert_eq!((id, said), ("call-7".into(), format!("{SAID}{SIGN_IN_AGAIN}")));
    assert_eq!((listed.status, listed.body.as_str()), (StatusCode::OK, LIST));
    assert_eq!(*asks.lock(), [asking(FIRST)]);
    assert_eq!(server.heard().len(), 2, "what it refused isn't called again");
}

/// What the hub made of it, as a newer hub's that this worker doesn't know too, and that it couldn't be told, or is older.
#[tokio::test]
async fn the_agent_is_told_what_the_person_can_do_about_it() {
    let server = refusing_calls(Some(WANTS)).await;
    let refused = |code| Err(ApiError::new(code, "made-up"));
    let cases = [
        (Ok(AskReply::ToolAccess(ToolAccess::NotAskedFor)), NO_MORE),
        (
            Ok(AskReply::ToolAccess(ToolAccess::OtherToken)),
            "Its sign-in has changed since, so this may work if it's tried again.",
        ),
        (Ok(AskReply::ToolAccess(ToolAccess::Unknown)), SIGN_IN_AGAIN),
        (refused(ErrorCode::Unsupported), SIGN_IN_AGAIN),
        (refused(ErrorCode::Unavailable), SIGN_IN_AGAIN),
    ];

    for (answer, then) in cases {
        let set = with(&server, "made-up-first", FIRST);
        let (agent, _asks) = answered_by(set, move |_| answer.clone()).await;

        let (_, said) = agent.post(CALL).await.error();

        assert_eq!(said, format!("{SAID}{then}"));
    }
}

#[tokio::test]
async fn a_challenge_that_names_no_scope_is_said_to_the_agent_and_the_hub_isnt_asked() {
    for challenge in [r#"Bearer error="insufficient_scope""#, r#"Bearer error="insufficient_scope", scope="\" é""#] {
        let server = refusing_calls(Some(challenge)).await;
        let set = with(&server, "made-up-first", FIRST);
        let (agent, asks) = answered_by(set, |_| Ok(SIGNS_IN_AGAIN)).await;

        let (_, said) = agent.post(CALL).await.error();

        assert_eq!(said, format!("{SAID}{NO_MORE}"));
        assert_eq!(*asks.lock(), []);
    }
}

/// A 403 that doesn't say that's why, and one to what isn't an access token the hub gave.
#[tokio::test]
async fn no_other_403_is_told_to_the_hub() {
    let server = refusing_calls(Some(WANTS)).await;
    let plain = refusing_calls(Some(r#"Bearer scope="made-up""#)).await;
    let mut newer = with(&server, "made-up-first", FIRST);
    newer.sign_in = Some(ServerSignIn::Unknown);

    for set in [with(&plain, "made-up-first", FIRST), as_set(&server), newer] {
        let (agent, asks) = answered_by(set, |_| Ok(SIGNS_IN_AGAIN)).await;

        let (_, said) = agent.post(CALL).await.error();

        assert!(said.starts_with("Linear (Northwind) refused the "), "{said}");
        assert_eq!(*asks.lock(), []);
    }
}

/// Its token ended as it was called, and the hub's other one is the one that isn't allowed it.
#[tokio::test]
async fn the_token_thats_said_to_lack_it_is_the_one_the_server_knew() {
    let server = StandIn::answering(|asked| {
        let knows = asked.headers["authorization"] == "Bearer made-up-second";
        let status = if knows { StatusCode::FORBIDDEN } else { StatusCode::UNAUTHORIZED };
        reply(status, Some(WANTS), asked)
    })
    .await;
    let renewed = with(&server, "made-up-second", FIRST.next());
    let (agent, asks) = answered_by(with(&server, "made-up-first", FIRST), move |ask| {
        let renews = matches!(ask, WorkerAsk::RenewToolServer(_));
        Ok(if renews { AskReply::ToolServer(renewed.clone()) } else { SIGNS_IN_AGAIN })
    })
    .await;

    let (_, said) = agent.post(CALL).await.error();

    assert_eq!(said, format!("{SAID}{SIGN_IN_AGAIN}"));
    assert_eq!(*asks.lock(), [renewing(FIRST), asking(FIRST.next())]);
}
