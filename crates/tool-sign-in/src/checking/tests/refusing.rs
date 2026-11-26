//! What's made of a server that lists no tools, none of which is what it says.

use http::{Response, StatusCode};
use http_body_util::{Either, Full};
use protocol::client::tool_server_checks::NoToolList;
use serde_json::{Value, json};

use super::{Heard, Reply, StandIn, as_json, as_stream, listed, status};

fn error() -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32603, "message": "made up"}})
}

fn empty() -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "result": {}})
}

fn page(code: u16) -> Reply {
    let page = Response::builder().status(code).header("content-type", "text/html");
    page.body(Either::Left(Full::from("<h1>Northwind</h1>"))).unwrap()
}

/// What a server that answers everything with `answer` comes to, asked with a key and with none.
async fn always(answer: fn() -> Reply) -> [Result<u32, NoToolList>; 2] {
    let server = StandIn::answering(move |_| answer()).await;
    [server.with_key().await, server.with_none().await]
}

#[tokio::test]
async fn a_status_says_why_a_server_wouldnt_answer() {
    let either = |why| [Err(why); 2];

    let unauthorized = [Err(NoToolList::KeyRefused { status: 401 }), Err(NoToolList::WantsSignInOrKey)];
    assert_eq!(always(|| status(401)).await, unauthorized);
    let forbidden = [Err(NoToolList::KeyRefused { status: 403 }), Err(NoToolList::Forbidden)];
    assert_eq!(always(|| status(403)).await, forbidden);
    assert_eq!(always(|| status(404)).await, either(NoToolList::NotFound));
    assert_eq!(always(|| status(429)).await, either(NoToolList::TooManyRequests));
    assert_eq!(always(|| status(500)).await, either(NoToolList::Failing { status: 500 }));
    assert_eq!(always(|| page(503)).await, either(NoToolList::Failing { status: 503 }));
}

#[tokio::test]
async fn what_answers_and_isnt_mcp_is_said_not_to_be() {
    let answers: [fn() -> Reply; 8] = [
        || page(200),
        || page(400),
        || page(402),
        || status(405),
        || status(204),
        || as_json(&json!({"error": "made up"})),
        || as_json(&json!({"jsonrpc": "1.0", "result": {"tools": []}})),
        || as_json(&json!("made up")),
    ];
    for (at, answer) in answers.into_iter().enumerate() {
        assert_eq!(always(answer).await, [Err(NoToolList::NotMcp); 2], "{at}");
    }
}

/// As a client of this version, and after a handshake.
#[tokio::test]
async fn a_server_that_connects_and_wont_list_its_tools_is_said_to() {
    let refuses: [fn() -> Reply; 5] = [
        || as_json(&error()),
        || as_stream(&error()),
        || as_json(&empty()),
        || page(400),
        || {
            let mut reply = as_json(&listed());
            *reply.status_mut() = StatusCode::NOT_FOUND;
            reply
        },
    ];
    for connects_to in ["server/discover", "initialize"] {
        for refuses in refuses {
            let answer = move |heard: &Heard| match heard.asks() {
                asks if asks == connects_to => as_json(&empty()),
                "tools/list" => refuses(),
                _ => status(400),
            };
            let server = StandIn::answering(answer).await;

            assert_eq!(server.with_none().await, Err(NoToolList::WontList), "{connects_to}");
        }
    }
}

#[tokio::test]
async fn a_server_that_refuses_a_handshake_wont_connect() {
    let answers: [fn() -> Reply; 2] = [
        || as_json(&error()),
        || {
            let mut reply = as_json(&error());
            *reply.status_mut() = StatusCode::BAD_REQUEST;
            reply
        },
    ];
    for answer in answers {
        assert_eq!(always(answer).await, [Err(NoToolList::WontConnect); 2]);
    }
}

/// Such as one that lists what it is to anyone, and its tools to no one without a key.
#[tokio::test]
async fn a_refusal_counts_wherever_it_comes() {
    for refuses in ["server/discover", "initialize", "tools/list"] {
        let answer = move |heard: &Heard| match heard.asks() {
            asks if asks == refuses => status(401),
            "server/discover" if refuses == "initialize" => status(400),
            _ => as_json(&empty()),
        };
        let server = StandIn::answering(answer).await;

        assert_eq!(server.with_key().await, Err(NoToolList::KeyRefused { status: 401 }), "{refuses}");
        assert_eq!(server.asked().last().unwrap(), refuses, "and nothing more is asked");
    }
}
