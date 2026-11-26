use std::sync::Arc;
use std::time::Duration;

use http_body_util::{BodyExt, Either, Full};
use hyper::body::Bytes;
use hyper::{Method, Response, StatusCode};
use parking_lot::Mutex;
use protocol::worker::ProxiedServer;
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::{Agent, Answer, Events, KEY, Reply, StandIn, as_set};

const LIST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
const LISTED: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;

fn reply(status: u16, headers: &[(&'static str, &str)], body: &'static str) -> Reply {
    let mut reply = Response::builder().status(status);
    for (name, value) in headers {
        reply = reply.header(*name, *value);
    }
    reply.body(Either::Left(Full::from(body))).unwrap()
}

/// Each of the transport's methods, with the headers of MCP's own that go each way and none of the rest.
#[tokio::test]
async fn what_an_agent_sends_is_passed_on_with_the_servers_key_and_the_answer_as_it_came() {
    let server = StandIn::answering(|asked| {
        let headers = [
            ("content-type", "application/json"),
            ("mcp-session-id", "session-1"),
            ("set-cookie", "a=b"),
            ("www-authenticate", "Bearer"),
            ("x-of-its-own", "1"),
        ];
        match asked.method {
            Method::POST => reply(200, &headers, LISTED),
            _ => reply(405, &[("allow", "POST")], "no stream"),
        }
    })
    .await;
    let agent = Agent::with(vec![as_set(&server)]).await;
    let path = agent.path().await;
    let sent = [
        ("content-type", "application/json"),
        ("accept", "application/json, text/event-stream"),
        ("mcp-protocol-version", "2025-11-25"),
        ("mcp-session-id", "session-1"),
        ("mcp-method", "tools/list"),
        ("last-event-id", "7"),
        ("cookie", "c=d"),
        ("proxy-authorization", "Basic x"),
        ("accept-encoding", "gzip"),
    ];

    let listed = Answer::of(agent.request(Method::POST, &path, &sent, LIST).await).await;
    let streamed = Answer::of(agent.request(Method::GET, &path, &sent, "").await).await;
    let ended = Answer::of(agent.request(Method::DELETE, &path, &sent, "").await).await;

    assert_eq!((listed.status, listed.body.as_str()), (StatusCode::OK, LISTED));
    assert_eq!((listed.header("content-type"), listed.header("mcp-session-id")), ("application/json", "session-1"));
    for dropped in ["set-cookie", "www-authenticate", "x-of-its-own"] {
        assert!(!listed.headers.contains_key(dropped), "{listed:?}");
    }
    assert_eq!((streamed.status, streamed.body.as_str()), (StatusCode::METHOD_NOT_ALLOWED, "no stream"));
    assert_eq!((ended.status, ended.header("allow")), (StatusCode::METHOD_NOT_ALLOWED, "POST"));
    let heard = server.heard();
    assert_eq!(
        heard.iter().map(|asked| &asked.method).collect::<Vec<_>>(),
        [Method::POST, Method::GET, Method::DELETE]
    );
    assert_eq!(heard[0].body, LIST);
    for asked in heard {
        assert_eq!(asked.path, "/mcp?query=made-up-query");
        assert_eq!(asked.headers["host"], format!("127.0.0.1:{}", server.port));
        assert_eq!(asked.headers["authorization"], KEY);
        for (name, value) in &sent[..6] {
            assert_eq!(asked.headers[*name], *value);
        }
        for dropped in ["cookie", "proxy-authorization", "accept-encoding"] {
            assert!(!asked.headers.contains_key(dropped), "{asked:?}");
        }
    }
}

/// A server with no header is sent none, the session's token included, and is sent the agent's own. How a server's
/// own takes the place of the agent's of its name is `tool_server_http`'s
/// `a_servers_own_header_goes_with_each_request_over_any_of_its_name`.
#[tokio::test]
async fn a_server_with_no_header_is_sent_none() {
    let server = StandIn::answering(|_| reply(202, &[], "")).await;
    let agent = Agent::with(vec![ProxiedServer { header: None, ..as_set(&server) }]).await;

    let path = agent.path().await;
    agent.request(Method::POST, &path, &[("mcp-key", "the agent's")], LIST).await;

    let heard = server.heard();
    assert_eq!(heard[0].headers["mcp-key"], "the agent's");
    assert!(!heard[0].headers.contains_key("authorization"), "{heard:?}");
}

/// Each event as the server sends it, and once the agent has hung up, the server's end of it goes too.
#[tokio::test]
async fn events_arrive_as_they_are_sent_and_the_stream_ends_with_the_agents() {
    let (events, sent) = mpsc::channel(1);
    let sent = Mutex::new(Some(sent));
    let server = StandIn::answering(move |_| {
        let body = Events(sent.lock().take().unwrap());
        Response::builder().header("content-type", "text/event-stream").body(body).unwrap()
    })
    .await;
    let agent = Agent::with(vec![as_set(&server)]).await;
    let within = Duration::from_secs(10);

    let path = agent.path().await;
    let mut stream = timeout(within, agent.request(Method::GET, &path, &[], "")).await.expect("the stream's headers");
    for event in ["data: 1\n\n", "data: 2\n\n"] {
        events.send(Bytes::from_static(event.as_bytes())).await.unwrap();
        let frame = timeout(within, stream.frame()).await.expect("the event, while the stream is open").unwrap();
        assert_eq!(frame.unwrap().into_data().unwrap(), event);
    }
    assert_eq!(stream.headers()["content-type"], "text/event-stream");
    drop(stream);

    timeout(within, events.closed()).await.expect("the server's end of the stream to end");
}

/// Every status but a refusal or a redirect is the server's own to say, such as 404 for a session it has ended.
#[tokio::test]
async fn a_status_is_passed_on() {
    let status = Arc::new(Mutex::new(200));
    let said = status.clone();
    let server = StandIn::answering(move |_| reply(*said.lock(), &[], "said")).await;
    let agent = Agent::with(vec![as_set(&server)]).await;

    for code in [200, 202, 400, 404, 429, 500] {
        *status.lock() = code;

        let answer = agent.post(LIST).await;

        assert_eq!((answer.status.as_u16(), answer.body.as_str()), (code, "said"));
    }
}

/// One followed, with the key, and one that isn't, which the agent is told of. Which are followed is
/// `tool_server_http`'s `a_redirect_is_followed_only_within_its_origin`.
#[tokio::test]
async fn a_redirect_is_followed_only_within_its_origin() {
    let to = Arc::new(Mutex::new(String::new()));
    let told = to.clone();
    let server = StandIn::answering(move |asked| {
        let location = told.lock().clone();
        if asked.path == "/moved" { reply(200, &[], "moved") } else { reply(307, &[("location", &location)], "") }
    })
    .await;
    let agent = Agent::with(vec![as_set(&server)]).await;

    *to.lock() = "/moved".into();
    let followed = agent.post(LIST).await;
    *to.lock() = format!("http://localhost:{}/moved", server.port);
    let (_, not_followed) = agent.post(LIST).await.error();

    assert_eq!(followed.body, "moved");
    let heard = server.heard();
    assert_eq!(
        (&heard[1].method, &heard[1].body[..], heard[1].header("authorization")),
        (&Method::POST, LIST.as_bytes(), Some(KEY))
    );
    assert!(not_followed.contains("redirected the request"), "{not_followed}");
    assert_eq!(server.heard().len(), 3, "{:?}", server.heard());
}
