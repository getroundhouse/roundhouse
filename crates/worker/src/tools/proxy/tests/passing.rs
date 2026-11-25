use std::sync::Arc;
use std::time::Duration;

use http_body_util::{BodyExt, Either, Full};
use hyper::body::Bytes;
use hyper::{Method, Response, StatusCode};
use parking_lot::Mutex;
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::stand_in::{KEY, header};
use super::{Agent, Answer, Events, Reply, StandIn};

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
    let agent = Agent::with(vec![server.as_set()]).await;
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

/// A server with no header is sent none, the session's token included, and one's own header takes the place of the
/// agent's of its name.
#[tokio::test]
async fn a_server_is_sent_only_the_header_it_has() {
    let server = StandIn::answering(|_| reply(202, &[], "")).await;
    let mut own = server.as_set();
    own.header.as_mut().unwrap().name = header("Mcp-Key");
    let mut keyless = server.as_set();
    keyless.header = None;

    for set in [own, keyless] {
        let agent = Agent::with(vec![set]).await;
        let path = agent.path().await;
        agent.request(Method::POST, &path, &[("mcp-key", "the agent's")], LIST).await;
    }

    let heard = server.heard();
    assert_eq!(heard[0].headers["mcp-key"], KEY);
    assert_eq!(heard[0].headers.get_all("mcp-key").iter().count(), 1);
    assert_eq!(heard[1].headers["mcp-key"], "the agent's");
    assert!(heard.iter().all(|asked| !asked.headers.contains_key("authorization")), "{heard:?}");
}

/// Each event as the server sends it, and once the agent has hung up, the server's end of it goes too.
#[tokio::test]
async fn events_arrive_as_they_are_sent_and_the_stream_ends_with_the_agents() {
    let (events, sent) = mpsc::channel(1);
    let sent = Mutex::new(Some(sent));
    let server = StandIn::answering(move |_| {
        let body = Events(sent.lock().take().unwrap());
        Response::builder().header("content-type", "text/event-stream").body(Either::Right(body)).unwrap()
    })
    .await;
    let agent = Agent::with(vec![server.as_set()]).await;
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
    let agent = Agent::with(vec![server.as_set()]).await;

    for code in [200, 202, 400, 404, 429, 500] {
        *status.lock() = code;

        let answer = agent.post(LIST).await;

        assert_eq!((answer.status.as_u16(), answer.body.as_str()), (code, "said"));
    }
}

/// Only within the origin the key is for, and only one that keeps the method and the body.
#[tokio::test]
async fn a_redirect_is_followed_only_within_its_origin() {
    let elsewhere = StandIn::answering(|_| reply(200, &[], "elsewhere")).await;
    let other = elsewhere.port;
    let to = Arc::new(Mutex::new((307, String::new())));
    let told = to.clone();
    let server = StandIn::answering(move |asked| {
        let (status, location) = told.lock().clone();
        if asked.path == "/moved" { reply(200, &[], "moved") } else { reply(status, &[("location", &location)], "") }
    })
    .await;
    let agent = Agent::with(vec![server.as_set()]).await;
    let here = server.port;
    let followed = [(307, "/moved".into()), (308, "/moved".into()), (308, format!("http://127.0.0.1:{here}/moved"))];
    let not_followed = [
        (307, format!("http://127.0.0.1:{other}/moved")),
        (308, format!("http://localhost:{here}/moved")),
        (307, format!("https://127.0.0.1:{here}/moved")),
        (307, format!("//127.0.0.1:{other}/moved")),
        (302, "/moved".into()),
        (303, "/moved".into()),
        (307, "moved".into()),
        // In circles.
        (307, "/mcp".into()),
    ];

    for redirect in followed {
        *to.lock() = redirect.clone();
        assert_eq!(agent.post(LIST).await.body, "moved", "{redirect:?}");
        let last = server.heard().pop().unwrap();
        assert_eq!((last.method, last.body), (Method::POST, Bytes::from(LIST)), "{redirect:?}");
        assert_eq!(last.headers["authorization"], KEY);
    }
    for redirect in not_followed {
        *to.lock() = redirect.clone();
        let (_, said) = agent.post(LIST).await.error();
        assert!(said.contains("redirected the request"), "{redirect:?}: {said}");
    }
    assert!(elsewhere.heard().is_empty(), "the key left its origin: {:?}", elsewhere.heard());
}
