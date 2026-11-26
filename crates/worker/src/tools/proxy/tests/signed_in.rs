//! A server that's signed in to, whose access token ends while an agent is calling it.

use std::sync::Arc;

use http_body_util::{Either, Full};
use hyper::{Response, StatusCode};
use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode};
use protocol::worker::{AskReply, ProxiedServer, RenewToolServer, ServerHeader, ServerSignIn, TokenNumber, WorkerAsk};

use super::stand_in::Heard;
use super::{Agent, Reply, StandIn};

pub(super) const CALL: &str = r#"{"jsonrpc":"2.0","id":"call-7","method":"tools/call","params":{"name":"list"}}"#;
pub(super) const FIRST: TokenNumber = TokenNumber::FIRST;
const REFUSED: &str = "Linear (Northwind) refused the sign-in Roundhouse has to it (401). Sign in to it again from Settings › MCP servers.";
const NEEDS_SIGN_IN: &str = "Linear (Northwind) needs you to sign in to it. Sign in from Settings › MCP servers.";

/// Takes the second token alone, and answers a call with its own body.
async fn taking_the_second() -> StandIn {
    StandIn::answering(|asked| {
        let takes = asked.headers["authorization"] == "Bearer made-up-second";
        let status = if takes { StatusCode::OK } else { StatusCode::UNAUTHORIZED };
        reply(status, asked)
    })
    .await
}

fn reply(status: StatusCode, asked: &Heard) -> Reply {
    Response::builder().status(status).body(Either::Left(Full::new(asked.body.clone()))).unwrap()
}

pub(super) fn with(server: &StandIn, access_token: &str, token: TokenNumber) -> ProxiedServer {
    let sign_in = Some(ServerSignIn::Works { token });
    ProxiedServer { header: Some(ServerHeader::bearer(access_token)), sign_in, ..server.as_set() }
}

fn needing_sign_in(server: &StandIn) -> ProxiedServer {
    ProxiedServer { header: None, sign_in: Some(ServerSignIn::NeedsSignIn), ..server.as_set() }
}

/// An agent whose server is `set`, and its worker's asks, each answered with `answer`.
async fn agent(set: ProxiedServer, answer: Result<ProxiedServer, ApiError>) -> (Agent, Arc<Mutex<Vec<WorkerAsk>>>) {
    answered_by(set, move |_| answer.clone().map(AskReply::ToolServer)).await
}

/// An agent whose server is `set`, and its worker's asks, each answered as `answer` says.
pub(super) async fn answered_by(
    set: ProxiedServer,
    answer: impl Fn(&WorkerAsk) -> Result<AskReply, ApiError> + Send + 'static,
) -> (Agent, Arc<Mutex<Vec<WorkerAsk>>>) {
    let asks = Arc::default();
    let asked: Arc<Mutex<Vec<WorkerAsk>>> = Arc::clone(&asks);
    let agent = Agent::whose_hub(vec![set], move |ask| {
        let answer = answer(&ask);
        asked.lock().push(ask);
        answer.into()
    })
    .await;
    (agent, asks)
}

pub(super) fn renewing(refused: TokenNumber) -> WorkerAsk {
    WorkerAsk::RenewToolServer(RenewToolServer { server: "t-1".into(), refused })
}

#[tokio::test]
async fn a_call_whose_token_is_refused_is_made_once_more_with_the_hubs_other_and_the_agent_never_knows() {
    let server = taking_the_second().await;
    let set = with(&server, "made-up-first", FIRST);
    let (agent, asks) = agent(set, Ok(with(&server, "made-up-second", FIRST.next()))).await;

    let answer = agent.post(CALL).await;

    assert_eq!((answer.status, answer.body.as_str()), (StatusCode::OK, CALL));
    assert_eq!(*asks.lock(), [renewing(FIRST)]);
    let bodies: Vec<_> = server.heard().into_iter().map(|heard| heard.body).collect();
    assert_eq!(bodies, [CALL, CALL], "once with each");
}

/// The hub has the same token still, or couldn't be asked, as one that's older, or it has another that's refused too.
#[tokio::test]
async fn an_agent_is_told_its_server_refused_the_sign_in_once_no_other_token_works() {
    let server = taking_the_second().await;
    let set = || with(&server, "made-up-first", FIRST);
    let older = Err(ApiError::new(ErrorCode::Unsupported, "made-up"));
    let cases = [(Ok(set()), 1), (older, 1), (Ok(with(&server, "made-up-other", FIRST.next())), 2)];
    let mut calls = 0;

    for (answer, called) in cases {
        let (agent, asks) = agent(set(), answer).await;

        let (_, said) = agent.post(CALL).await.error();

        calls += called;
        assert_eq!((said.as_str(), server.heard().len()), (REFUSED, calls));
        assert_eq!(*asks.lock(), [renewing(FIRST)], "asked once");
    }
}

#[tokio::test]
async fn an_agent_is_told_to_have_the_person_sign_in_when_the_hub_says_so_and_before_anything_is_called() {
    let server = taking_the_second().await;
    let (asking, asks) = agent(with(&server, "made-up-first", FIRST), Ok(needing_sign_in(&server))).await;
    let (_, as_its_called) = asking.post(CALL).await.error();
    let called = server.heard().len();
    let (set, others) = agent(needing_sign_in(&server), Ok(needing_sign_in(&server))).await;

    let (_, as_its_set) = set.post(CALL).await.error();

    assert_eq!((as_its_called.as_str(), as_its_set.as_str()), (NEEDS_SIGN_IN, NEEDS_SIGN_IN));
    assert_eq!((called, server.heard().len()), (1, 1));
    assert_eq!((asks.lock().len(), others.lock().len()), (1, 0));
}

/// A 403 that wants no more access says it knew who was asking, a server with a key has no other, and nothing says
/// which a newer hub's is.
#[tokio::test]
async fn nothing_else_a_server_refuses_is_asked_about() {
    let forbidding = StandIn::answering(|asked| reply(StatusCode::FORBIDDEN, asked)).await;
    let server = taking_the_second().await;
    let newer = ProxiedServer { sign_in: Some(ServerSignIn::Unknown), ..with(&server, "made-up-first", FIRST) };
    let cases = [with(&forbidding, "made-up-first", FIRST), server.as_set(), newer];

    for set in cases {
        let (agent, asks) = agent(set, Ok(with(&server, "made-up-second", FIRST.next()))).await;

        let (_, said) = agent.post(CALL).await.error();

        assert!(said.starts_with("Linear (Northwind) refused the "), "{said}");
        assert_eq!(*asks.lock(), []);
    }
}
