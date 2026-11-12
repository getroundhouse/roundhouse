use std::sync::Arc;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::header::{AUTHORIZATION, HOST, HeaderMap, HeaderValue, ORIGIN};
use hyper::{Method, StatusCode, Uri};
use protocol::worker::ToolsToken;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::*;
use crate::asks::Asks;

const PORT: u16 = 50_123;

fn tools() -> Tools {
    let (frames, _) = mpsc::channel(8);
    let tools = Tools::new(Arc::new(Asks::new(frames)));
    tools.admit(ToolsToken::new("token-s"), "s".into());
    tools
}

/// The headers the CLI sends with `tools/list`.
fn sent() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(HOST, HeaderValue::from_static("127.0.0.1:50123"));
    headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer token-s"));
    headers.insert(VERSION_HEADER, HeaderValue::from_static("2026-07-28"));
    headers.insert(METHOD_HEADER, HeaderValue::from_static("tools/list"));
    headers
}

fn list() -> Bytes {
    let meta = json!({ "io.modelcontextprotocol/protocolVersion": "2026-07-28" });
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": { "_meta": meta } });
    Bytes::from(body.to_string())
}

async fn post(tools: &Tools, headers: &HeaderMap, body: Bytes) -> (StatusCode, Bytes) {
    send(tools, &Method::POST, "/mcp", headers, body).await
}

async fn send(tools: &Tools, method: &Method, path: &str, headers: &HeaderMap, body: Bytes) -> (StatusCode, Bytes) {
    let uri: Uri = path.parse().unwrap();
    let response = respond(tools, PORT, method, &uri, headers, Full::new(body)).await;
    let status = response.status();
    (status, response.into_body().collect().await.unwrap().to_bytes())
}

#[tokio::test]
async fn a_request_from_an_agent_of_this_workers_is_answered_as_json() {
    let tools = tools();

    let (status, body) = post(&tools, &sent(), list()).await;

    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["result"]["tools"][0]["name"], "get_pull_request");
}

/// A body that fails the test if anything reads it.
struct NotToBeRead;

impl hyper::body::Body for NotToBeRead {
    type Data = Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
        panic!("the body was read");
    }
}

#[tokio::test]
async fn a_request_without_a_token_of_this_workers_is_refused_before_its_body_is_read() {
    let tools = tools();
    let mut wrong = sent();
    wrong.insert(AUTHORIZATION, HeaderValue::from_static("Bearer someone-else"));
    let mut other_scheme = sent();
    other_scheme.insert(AUTHORIZATION, HeaderValue::from_static("Basic token-s"));
    let mut none = sent();
    none.remove(AUTHORIZATION);
    let uri: Uri = "/mcp".parse().unwrap();

    for headers in [wrong, other_scheme, none] {
        let response = respond(&tools, PORT, &Method::POST, &uri, &headers, NotToBeRead).await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers().get(hyper::header::WWW_AUTHENTICATE).unwrap(), "Bearer");
    }
}

#[tokio::test]
async fn a_request_from_a_web_page_or_by_another_name_is_forbidden() {
    let tools = tools();
    let mut from_a_page = sent();
    from_a_page.insert(ORIGIN, HeaderValue::from_static("https://example.com"));
    let mut by_another_name = sent();
    by_another_name.insert(HOST, HeaderValue::from_static("rebound.example.com:50123"));
    let mut another_port = sent();
    another_port.insert(HOST, HeaderValue::from_static("127.0.0.1:80"));
    let mut localhost = sent();
    localhost.insert(HOST, HeaderValue::from_static("LocalHost:50123"));

    for headers in [from_a_page, by_another_name, another_port] {
        assert_eq!(post(&tools, &headers, list()).await.0, StatusCode::FORBIDDEN, "{headers:?}");
    }
    assert_eq!(post(&tools, &localhost, list()).await.0, StatusCode::OK);
}

#[tokio::test]
async fn only_posts_to_its_one_path_are_taken() {
    let tools = tools();

    let elsewhere = send(&tools, &Method::POST, "/other", &sent(), list()).await.0;
    let get = send(&tools, &Method::GET, "/mcp", &sent(), Bytes::new()).await.0;
    let delete = send(&tools, &Method::DELETE, "/mcp", &sent(), Bytes::new()).await.0;

    assert_eq!(elsewhere, StatusCode::NOT_FOUND);
    assert_eq!((get, delete), (StatusCode::METHOD_NOT_ALLOWED, StatusCode::METHOD_NOT_ALLOWED));
}

#[tokio::test]
async fn a_body_too_large_is_refused() {
    let tools = tools();

    let (status, _) = post(&tools, &sent(), Bytes::from(vec![b' '; MAX_BODY_BYTES + 1])).await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn each_refusal_has_streamable_https_status() {
    let tools = tools();
    let mut mismatched = sent();
    mismatched.insert(METHOD_HEADER, HeaderValue::from_static("tools/call"));
    let notification = Bytes::from_static(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    let unknown = json!({ "jsonrpc": "2.0", "id": 2, "method": "nothing/here",
        "params": { "_meta": { "io.modelcontextprotocol/protocolVersion": "2026-07-28" } } });
    let mut asking_unknown = sent();
    asking_unknown.insert(METHOD_HEADER, HeaderValue::from_static("nothing/here"));

    assert_eq!(post(&tools, &mismatched, list()).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(post(&tools, &sent(), notification).await, (StatusCode::ACCEPTED, Bytes::new()));
    assert_eq!(post(&tools, &asking_unknown, Bytes::from(unknown.to_string())).await.0, StatusCode::NOT_FOUND);
}

#[test]
fn a_name_that_isnt_plain_ascii_is_read_from_base64() {
    assert_eq!(decoded("get_pull_request"), Said::Value("get_pull_request".into()));
    assert_eq!(decoded("=?base64?SGVsbG8sIOS4lueVjA==?="), Said::Value("Hello, 世界".into()));
    assert_eq!(decoded("=?base64?not base64?="), Said::Unreadable);
}
