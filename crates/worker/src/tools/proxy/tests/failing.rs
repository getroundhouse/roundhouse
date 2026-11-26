use std::net::Ipv4Addr;
use std::time::Duration;

use http_body_util::{Either, Full};
use hyper::{Method, Response, StatusCode};
use protocol::model::ToolServerId;
use protocol::worker::ProxiedServer;
use serde_json::json;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

use super::stand_in::{KEY, url};
use super::{Agent, Answer, Reply, StandIn};
use crate::tools::proxy::path::path_of;
use crate::tools::proxy::server_in;
use crate::tools::proxy::upstream::CONNECT_WITHIN;

const CALL: &str = r#"{"jsonrpc":"2.0","id":"call-7","method":"tools/call","params":{"name":"list"}}"#;
const NOTIFICATION: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
const REPLY: &str = r#"{"jsonrpc":"2.0","id":3,"result":{}}"#;

fn refusing(status: StatusCode) -> Reply {
    // A server that gives its refusal away, with the key it was sent.
    let refusal = Response::builder().status(status).header("www-authenticate", "Bearer resource_metadata=\"x\"");
    refusal.body(Either::Left(Full::from(KEY))).unwrap()
}

/// What the agent is told of a call to `server`, and that nothing else it could send is answered with what a CLI takes
/// for its own sign-in failing.
async fn told(server: ProxiedServer) -> String {
    let agent = Agent::with(vec![server]).await;

    let (id, said) = agent.post(CALL).await.error();
    let without_an_id = [agent.post(NOTIFICATION).await, agent.post(REPLY).await, agent.get().await];

    assert_eq!(id, json!("call-7"));
    for answer in without_an_id {
        assert_eq!((answer.status, &answer.body), (StatusCode::BAD_GATEWAY, &said), "{answer:?}");
        assert!(!answer.headers.contains_key("www-authenticate"), "{answer:?}");
    }
    said
}

#[tokio::test]
async fn a_server_that_refuses_is_said_to_have_rather_than_its_401_or_403_passed_on() {
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        let server = StandIn::answering(move |_| refusing(status)).await;
        let keyless = ProxiedServer { header: None, ..server.as_set() };
        let code = status.as_u16();

        let with_a_key = told(server.as_set()).await;
        let without = told(keyless).await;

        let refused = "Linear (Northwind) refused the key Roundhouse sent it";
        assert_eq!(with_a_key, format!("{refused} ({code}). Give it another in Settings › MCP servers."));
        let refused = "Linear (Northwind) refused a request without a key";
        assert_eq!(without, format!("{refused} ({code}). Give it one in Settings › MCP servers."));
    }
}

#[tokio::test]
async fn a_server_that_isnt_there_or_goes_before_it_answers_is_said_to_have() {
    let server = StandIn::answering(|_| refusing(StatusCode::OK)).await.as_set();
    // A port the system gives nobody who asks for any.
    let isnt_there = ProxiedServer { url: url("http://127.0.0.1:1/?made-up"), ..server.clone() };
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let hangs_up = ProxiedServer { url: url(&format!("http://127.0.0.1:{port}/?made-up")), ..server.clone() };
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _ = stream.read(&mut [0; 1024]).await;
        }
    });

    let unreachable = told(isnt_there).await;
    let lost = told(hangs_up).await;
    let unusable = told(ProxiedServer { url: serde_json::from_value(json!("made-up")).unwrap(), ..server }).await;

    assert_eq!(unreachable, "Linear (Northwind) couldn't be reached from this machine.");
    assert!(lost.starts_with("Linear (Northwind) closed the connection before it answered"), "{lost}");
    assert!(unusable.starts_with("Roundhouse can't use the URL or the header Linear (Northwind) has"), "{unusable}");
}

/// One that takes the connection and never agrees on TLS.
#[tokio::test]
async fn a_server_that_never_finishes_connecting_is_given_up_on() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = StandIn::answering(|_| refusing(StatusCode::OK)).await.as_set();
    let stuck = ProxiedServer { url: url(&format!("https://127.0.0.1:{port}/?made-up")), ..server };
    let agent = Agent::with(vec![stuck]).await;

    let asking = tokio::spawn(async move { agent.post(CALL).await.error().1 });
    let within = Duration::from_secs(10);
    let _held = tokio::time::timeout(within, listener.accept()).await.expect("a connection").unwrap();
    tokio::time::pause();
    tokio::time::advance(CONNECT_WITHIN).await;
    tokio::time::resume();

    let said = tokio::time::timeout(within, asking).await.expect("an answer").unwrap();
    assert_eq!(said, "Linear (Northwind) couldn't be reached from this machine.");
}

/// Before the server has answered, which is what ends a call that takes long, since the worker doesn't.
#[tokio::test]
async fn an_agent_that_hangs_up_ends_its_call_to_the_server() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = StandIn::answering(|_| refusing(StatusCode::OK)).await.as_set();
    let agent = Agent::with(vec![ProxiedServer { url: url(&format!("http://127.0.0.1:{port}/")), ..server }]).await;
    let within = Duration::from_secs(10);

    let calling = agent.raw(&Method::POST, &agent.path().await, CALL).await;
    let (mut asked, _) = tokio::time::timeout(within, listener.accept()).await.expect("the call").unwrap();
    let mut heard = Vec::new();
    while !heard.ends_with(CALL.as_bytes()) {
        let more = tokio::time::timeout(within, asked.read_buf(&mut heard)).await.expect("all of the call");
        assert_ne!(more.unwrap(), 0, "the call ended before the agent hung up");
    }
    drop(calling);

    let mut rest = Vec::new();
    tokio::time::timeout(within, asked.read_to_end(&mut rest)).await.expect("the call to end").unwrap();
    assert!(rest.is_empty());
}

/// By its ID, so a new header moves nothing, and an agent launched with a server that's since gone from the set is told
/// Roundhouse can't use it. The hub leaves out one whose key it can't read as well, so it isn't said to be removed.
#[tokio::test]
async fn a_server_keeps_its_path_as_its_header_changes_and_says_once_its_not_set() {
    let server = StandIn::answering(|_| refusing(StatusCode::OK)).await;
    let agent = Agent::with(vec![server.as_set()]).await;
    let path = agent.path().await;

    agent.tools.proxy.set(vec![ProxiedServer { header: None, ..server.as_set() }]);
    let changed = agent.path().await;
    agent.tools.proxy.set(vec![]);
    let removed = Answer::of(agent.request(Method::POST, &path, &[], CALL).await).await;

    assert_eq!(changed, path);
    let (id, said) = removed.error();
    assert_eq!(id, json!("call-7"));
    let not_set =
        "Roundhouse can't use this MCP server now. It may have been removed, or its key or sign-in can't be read.";
    assert_eq!(said, not_set);
    assert!(server.heard().is_empty());
}

/// A request, by its method, path and headers, and the status it's refused with.
type Refused = (Method, String, &'static [(&'static str, &'static str)], StatusCode);

/// As the worker's own tools are, before anything reaches the server.
#[tokio::test]
async fn only_a_request_from_one_of_this_workers_agents_reaches_a_server() {
    let server = StandIn::answering(|_| refusing(StatusCode::OK)).await;
    let agent = Agent::with(vec![server.as_set()]).await;
    let path = agent.path().await;
    let refused: [Refused; 9] = [
        (Method::POST, path.clone(), &[("authorization", "Bearer another-session's")], StatusCode::UNAUTHORIZED),
        (Method::GET, path.clone(), &[("authorization", "")], StatusCode::UNAUTHORIZED),
        (Method::POST, path.clone(), &[("origin", "https://example.com")], StatusCode::FORBIDDEN),
        (Method::POST, path.clone(), &[("host", "rebound.example.com")], StatusCode::FORBIDDEN),
        (Method::PUT, path.clone(), &[], StatusCode::METHOD_NOT_ALLOWED),
        (Method::GET, format!("/.well-known/oauth-protected-resource{path}"), &[], StatusCode::NOT_FOUND),
        (Method::GET, format!("{path}/.well-known/oauth-protected-resource"), &[], StatusCode::NOT_FOUND),
        (
            Method::GET,
            "/.well-known/oauth-authorization-server".into(),
            &[("authorization", "")],
            StatusCode::NOT_FOUND,
        ),
        (Method::POST, "/tool-servers/".into(), &[], StatusCode::NOT_FOUND),
    ];

    for (method, path, headers, status) in refused {
        let answer = Answer::of(agent.request(method, &path, headers, CALL).await).await;

        assert_eq!(answer.status, status, "{path} with {headers:?}");
    }
    assert!(server.heard().is_empty(), "{:?}", server.heard());
}

#[tokio::test]
async fn a_message_too_large_to_hold_is_refused_and_one_larger_than_the_workers_own_tools_take_isnt() {
    let server = StandIn::answering(|_| refusing(StatusCode::OK)).await;
    let agent = Agent::with(vec![ProxiedServer { header: None, ..server.as_set() }]).await;
    let path = agent.path().await;

    agent.request(Method::POST, &path, &[], vec![b' '; 2 * 1024 * 1024]).await;
    let refused = agent.request(Method::POST, &path, &[("expect", "100-continue")], vec![b' '; 16 * 1024 * 1024 + 1]);

    assert_eq!(refused.await.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(server.heard().iter().map(|asked| asked.body.len()).collect::<Vec<_>>(), [2 * 1024 * 1024]);
}

#[test]
fn any_id_names_a_path_that_reads_back_as_it() {
    for id in ["0b9c7d1e-5a52-4a0e-9d7c-0f6e2f6b8f11", "a/b", "..", "Ünï %2F"] {
        let path = path_of(&ToolServerId::from(id));

        assert_eq!(server_in(&path), Some(ToolServerId::from(id)), "{path}");
    }
}
