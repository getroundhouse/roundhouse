//! A server that takes calls and never answers them, which holds only so many of the connections the worker takes.

use std::iter::repeat_n;
use std::net::Ipv4Addr;
use std::time::Duration;

use hyper::{Method, StatusCode};
use logging::testing::Logs;
use protocol::model::ToolServerName;
use protocol::worker::ProxiedServer;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::stand_in::url;
use super::{Agent, Answer, StandIn};
use crate::tools::proxy::under_way::MOST_UNDER_WAY;
use crate::tools::server::MAX_CONNECTIONS;

const CALL: &str = r#"{"jsonrpc":"2.0","id":"call-7","method":"tools/call","params":{"name":"list"}}"#;
const NOTIFICATION: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
const TOO_MANY: &str = "Linear (Northwind) has 8 calls under way that it hasn't answered, so Roundhouse sends it no more until one of them ends.";

/// A server that takes each connection and never answers on it, and the server as the hub sets it.
async fn never_answering(within: Duration) -> (ProxiedServer, mpsc::UnboundedReceiver<()>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (taken, taking) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok(Ok((stream, _))) = timeout(within, listener.accept()).await {
            held.push(stream);
            let _ = taken.send(());
        }
    });
    let server = ProxiedServer {
        id: "t-1".into(),
        name: "linear_northwind".into(),
        label: ToolServerName::typed("Linear (Northwind)").unwrap(),
        url: url(&format!("http://127.0.0.1:{port}/mcp")),
        header: None,
        sign_in: None,
    };
    (server, taking)
}

/// Calls and streams of events it never begins, and then more calls than the worker takes connections, each on a
/// connection the agent keeps open, as a CLI that doesn't close one does. They're refused at once, past the limit, so
/// the worker's own tools still answer.
#[tokio::test]
async fn a_server_that_never_answers_is_sent_only_so_many_calls_and_the_workers_own_tools_still_answer() {
    let logs = Logs::capture();
    let within = Duration::from_secs(10);
    let (server, mut taking) = never_answering(within).await;
    let agent = Agent::with(vec![server]).await;
    let path = agent.path().await;
    let (mut stuck, mut kept) = (Vec::new(), Vec::new());
    for (method, body) in [(Method::GET, ""), (Method::POST, CALL)].into_iter().cycle().take(MOST_UNDER_WAY) {
        stuck.push(agent.raw(&method, &path, body).await);
        timeout(within, taking.recv()).await.expect("the call to reach the server");
    }

    let mut refused = Vec::new();
    for body in [NOTIFICATION].into_iter().chain([CALL; MAX_CONNECTIONS]) {
        let sent = timeout(within, agent.keeping(Method::POST, &path, &[], body)).await.expect("an answer at once");
        let Ok((answer, connection)) = sent else { break };
        refused.push(Answer::of(answer).await);
        kept.push(connection);
    }
    let tools = agent.request(Method::POST, "/mcp", &LISTING, list()).await;

    assert_eq!(tools.status(), StatusCode::OK);
    let [notification, calls @ ..] = &refused[..] else { panic!("{refused:?}") };
    assert_eq!(calls.len(), MAX_CONNECTIONS, "each call was answered");
    assert_eq!((notification.status, notification.body.as_str()), (StatusCode::BAD_GATEWAY, TOO_MANY));
    for call in calls {
        assert_eq!(call.error(), (json!("call-7"), TOO_MANY.to_string()));
    }
    assert!(taking.try_recv().is_err(), "no more reached the server");
    let logged = logs.text().lines().filter(|line| line.contains("tool_server.too_many_under_way")).count();
    assert_eq!(logged, 1, "it's logged once, since a CLI can go on asking");
}

/// One that answers each call with a stream, and never ends it, holds its call as one that never answers does. Its own
/// streams of events, which stay open for as long as each agent is connected, count only until they're begun.
#[tokio::test]
async fn a_call_counts_until_its_answer_ends_and_a_stream_of_events_until_its_begun() {
    let within = Duration::from_secs(10);
    let (to_calls, calls) = mpsc::unbounded_channel();
    let (to_streams, streams) = mpsc::unbounded_channel();
    let server = StandIn::answering(move |asked| {
        let (events, stream) = mpsc::channel(1);
        // Each answer ends once the test drops where it's sent from, and any after that at once.
        let _ = if asked.method == Method::GET { to_streams.send(events) } else { to_calls.send(events) };
        let answer = hyper::Response::builder().header("content-type", "text/event-stream");
        answer.body(http_body_util::Either::Right(super::Events(stream))).unwrap()
    })
    .await;
    let agent = Agent::with(vec![server.as_set()]).await;
    let path = agent.path().await;
    let mut answers = Vec::new();
    let asking = repeat_n((Method::GET, ""), MOST_UNDER_WAY + 1).chain(repeat_n((Method::POST, CALL), MOST_UNDER_WAY));
    for (method, body) in asking {
        let answer = timeout(within, agent.request(method, &path, &[], body)).await.expect("its headers");
        assert_eq!(answer.status(), StatusCode::OK);
        answers.push(answer);
    }

    let (_, said) = timeout(within, agent.post(CALL)).await.expect("an answer at once").error();
    drop(calls);
    let answered = timeout(within, agent.request(Method::POST, &path, &[], CALL)).await.expect("an answer");

    assert_eq!(said, TOO_MANY);
    assert_eq!(answered.status(), StatusCode::OK, "it was sent once the others ended");
    drop((answers, streams));
}

/// What an agent's CLI sends to list the worker's own tools.
const LISTING: [(&str, &str); 2] = [("mcp-protocol-version", "2026-07-28"), ("mcp-method", "tools/list")];

fn list() -> String {
    let meta = json!({ "io.modelcontextprotocol/protocolVersion": "2026-07-28" });
    let list: Value = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": { "_meta": meta } });
    list.to_string()
}
