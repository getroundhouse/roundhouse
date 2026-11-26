//! A server the person moves to another URL while an agent is connected to it.

use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Method, Response, StatusCode};
use parking_lot::Mutex;
use protocol::worker::AskReply;
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::signed_in::{FIRST, answered_by, with};
use super::{Agent, Answer, Events, StandIn, as_set};

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}"#;
const CALL: &str = r#"{"jsonrpc":"2.0","id":"call-7","method":"tools/call","params":{"name":"list"}}"#;
const LISTEN: &str = r#"{"jsonrpc":"2.0","id":"listen-1","method":"subscriptions/listen","params":{}}"#;

/// One that gives each agent the session `session`, and answers a message with itself.
async fn giving(session: &'static str) -> StandIn {
    StandIn::answering(move |asked| {
        let reply = Response::builder().header("mcp-session-id", session);
        reply.body(Full::new(asked.body.clone())).unwrap()
    })
    .await
}

async fn send(agent: &Agent, method: Method, session: &str, body: &'static str) -> Answer {
    let path = agent.path().await;
    Answer::of(agent.request(method, &path, &[("mcp-session-id", session)], body).await).await
}

/// Connected to `server`, in the session it gave.
async fn connected(server: &StandIn, session: &'static str) -> Agent {
    let agent = Agent::with(vec![as_set(server)]).await;
    let answer = send(&agent, Method::POST, "", INITIALIZE).await;
    assert_eq!((answer.status, answer.header("mcp-session-id")), (StatusCode::OK, session));
    agent
}

fn move_to(agent: &Agent, server: &StandIn) {
    agent.tools.proxy.set(vec![as_set(server)]);
}

/// As the hub has set it, in whatever session the agent sends. A server answers one it didn't give with 404, as MCP has
/// it, after which Claude and Codex each connect to it afresh and send the call again.
#[tokio::test]
async fn an_agents_call_goes_where_its_server_is_moved_at_once() {
    let (before, after) = (giving("before").await, giving("after").await);
    let agent = connected(&before, "before").await;

    move_to(&agent, &after);

    let called = send(&agent, Method::POST, "before", CALL).await;
    assert_eq!((called.status, called.body.as_str()), (StatusCode::OK, CALL));
    let heard = after.heard();
    let [heard] = &heard[..] else { panic!("{heard:?}") };
    assert_eq!((heard.headers["mcp-session-id"].to_str().unwrap(), &heard.body[..]), ("before", CALL.as_bytes()));
    assert_eq!(before.heard().len(), 1, "nor does it reach the old one");
}

/// Its messages come from the server before, and it's open for as long as the agent runs, whichever way its protocol
/// has it asked for. A call under way goes on. The same once the server is removed, or left out of the set.
#[tokio::test]
async fn a_servers_stream_ends_as_its_moved_or_removed_and_a_calls_answer_goes_on() {
    let after = giving("after").await;
    for removed in [false, true] {
        let (senders, streams): (Vec<_>, Vec<_>) = (0..3).map(|_| mpsc::channel(1)).unzip();
        let [get, listening, call] = <[_; 3]>::try_from(senders).unwrap();
        let streams = Mutex::new(streams.into_iter());
        let before = StandIn::answering(move |_| {
            Response::builder()
                .header("content-type", "text/event-stream")
                .body(Events(streams.lock().next().unwrap()))
                .unwrap()
        })
        .await;
        let agent = Agent::with(vec![as_set(&before)]).await;
        let path = agent.path().await;
        let got = agent.request(Method::GET, &path, &[], "").await;
        let listened = agent.request(Method::POST, &path, &[], LISTEN).await;
        let mut answer = agent.request(Method::POST, &path, &[], CALL).await;
        let within = Duration::from_secs(10);

        if removed {
            agent.tools.proxy.set(vec![]);
        } else {
            move_to(&agent, &after);
        }

        for (mut stream, servers) in [(got, get), (listened, listening)] {
            let ended = timeout(within, stream.frame()).await.expect("the stream to end");
            assert!(ended.is_none(), "{ended:?}");
            timeout(within, servers.closed()).await.expect("the server's end of it to end");
        }
        call.send(Bytes::from_static(b"data: 1\n\n")).await.unwrap();
        let event = timeout(within, answer.frame()).await.expect("the answer").unwrap().unwrap();
        assert_eq!(event.into_data().unwrap(), "data: 1\n\n");
    }
}

/// A call refused by the server before, whose sign-in the hub renews where it's moved to meanwhile, is made again there,
/// as any call made after the move is.
#[tokio::test]
async fn a_call_is_made_again_where_its_server_has_moved_as_its_sign_in_is_renewed() {
    let refusing =
        StandIn::answering(|_| Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::default()).unwrap())
            .await;
    let after = giving("after").await;
    let renewed = with(&after, "made-up-second", FIRST.next());
    let set = with(&refusing, "made-up-first", FIRST);
    let (agent, _asks) = answered_by(set, move |_| Ok(AskReply::ToolServer(renewed.clone()))).await;

    let called = agent.post(CALL).await;

    assert_eq!((called.status, called.body.as_str()), (StatusCode::OK, CALL));
    let heard: Vec<_> = after.heard().into_iter().map(|heard| heard.headers["authorization"].clone()).collect();
    assert_eq!(heard, ["Bearer made-up-second"], "with the hub's other token");
}
