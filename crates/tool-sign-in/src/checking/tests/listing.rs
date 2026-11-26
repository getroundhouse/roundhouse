//! What a server is asked, as a client of each version of MCP, and how it may answer.

use serde_json::{Value, json};

use super::{Heard, KEY, Reply, StandIn, as_json, as_stream, listed, status, with_key, with_none};

fn result(result: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "result": result})
}

fn error() -> Value {
    json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32601, "message": "made up"}})
}

fn discovered() -> Value {
    result(&json!({"supportedVersions": ["2026-07-28"], "capabilities": {"tools": {}}}))
}

/// The answer of a server on `2025-06-18` that gives `session`, if it gives one, and refuses what comes before the
/// handshake with `refuses`.
fn shakes_hands(
    session: Option<&'static str>,
    answer: fn(&Value) -> Reply,
    refuses: impl Fn() -> Reply + Send + Sync + 'static,
) -> impl Fn(&Heard) -> Reply + Send + Sync + 'static {
    move |heard| {
        let in_session = heard.header("mcp-session-id") == session;
        match heard.asks() {
            "initialize" => {
                let mut reply = answer(&result(&json!({"protocolVersion": "2025-06-18", "capabilities": {}})));
                if let Some(session) = session {
                    reply.headers_mut().insert("mcp-session-id", session.parse().unwrap());
                }
                reply
            }
            "notifications/initialized" if in_session => status(202),
            "tools/list" if in_session && heard.header("mcp-protocol-version") == Some("2025-06-18") => {
                answer(&listed())
            }
            "DELETE" if in_session => status(200),
            _ => refuses(),
        }
    }
}

/// Each with what that version has a client say of itself and of what it asks, and the key.
#[tokio::test]
async fn a_server_on_this_version_is_asked_what_it_is_and_then_for_its_tools_and_answers_either_way() {
    for answer in [as_json, as_stream] {
        let server = StandIn::answering(move |heard| match heard.asks() {
            "server/discover" => answer(&discovered()),
            "tools/list" => answer(&listed()),
            _ => status(400),
        })
        .await;

        let tools = with_key(&server).await;

        assert_eq!(tools, Ok(2));
        assert_eq!(server.asked(), ["server/discover", "tools/list"], "one page of them");
        for heard in server.heard() {
            assert_eq!((heard.method.as_str(), heard.path.as_str()), ("POST", "/mcp?query=made-up-query"));
            assert_eq!(heard.header("mcp-protocol-version"), Some("2026-07-28"));
            assert_eq!(heard.header("mcp-method"), Some(heard.asks()));
            assert_eq!(heard.header("accept"), Some("application/json, text/event-stream"));
            assert_eq!(heard.header("content-type"), Some("application/json"));
            assert_eq!(heard.header("authorization"), Some(KEY));
            let about = &heard.message["params"]["_meta"];
            assert_eq!(about["io.modelcontextprotocol/protocolVersion"], "2026-07-28");
            assert_eq!(about["io.modelcontextprotocol/clientInfo"]["name"], "roundhouse");
            assert_eq!(about["io.modelcontextprotocol/clientCapabilities"], json!({}));
        }
    }
}

/// However it refuses a version it doesn't know.
#[tokio::test]
async fn a_server_on_a_version_before_is_shaken_hands_with_and_its_session_ended() {
    let refuses: [fn() -> Reply; 5] = [
        || status(400),
        || status(404),
        || as_json(&error()),
        || as_stream(&error()),
        // To what isn't in a session.
        || status(202),
    ];
    for (refuses, answer) in refuses.into_iter().zip([as_json, as_stream].into_iter().cycle()) {
        let server = StandIn::answering(shakes_hands(Some("made-up-session"), answer, refuses)).await;

        let tools = with_key(&server).await;

        assert_eq!(tools, Ok(2));
        let asked = ["server/discover", "initialize", "notifications/initialized", "tools/list", "DELETE"];
        assert_eq!(server.asked(), asked);
        let heard = server.heard();
        let said = |name: &str| heard.iter().skip(1).map(|heard| heard.header(name)).collect::<Vec<_>>();
        let of_session = Some("made-up-session");
        assert_eq!(said("mcp-session-id"), [None, of_session, of_session, of_session]);
        let version = Some("2025-06-18");
        assert_eq!(said("mcp-protocol-version"), [None, version, version, version], "the one it chose");
        assert_eq!(said("mcp-method"), [None; 4]);
        assert_eq!(heard[1].message["params"]["protocolVersion"], "2025-11-25");
        assert!(heard.iter().all(|heard| heard.header("authorization") == Some(KEY)));
    }
}

#[tokio::test]
async fn a_server_that_gives_no_session_has_none_ended() {
    let server = StandIn::answering(shakes_hands(None, as_json, || status(405))).await;

    let tools = with_none(&server).await;

    assert_eq!(tools, Ok(2));
    assert_eq!(server.asked(), ["server/discover", "initialize", "notifications/initialized", "tools/list"]);
    assert!(server.heard().iter().all(|heard| heard.header("authorization").is_none()));
}

/// Whatever it does to be ended, and a server that says which version it chose in no way that can be sent back.
#[tokio::test]
async fn a_session_is_ended_whatever_came_of_it() {
    let failing = StandIn::answering(|heard| match heard.asks() {
        "initialize" => {
            let mut reply = as_json(&result(&json!({"protocolVersion": "made\nup"})));
            reply.headers_mut().insert("mcp-session-id", "made-up-session".parse().unwrap());
            reply
        }
        "tools/list" => as_json(&error()),
        _ => status(400),
    })
    .await;

    let refused = with_key(&failing).await;

    assert!(refused.is_err(), "{refused:?}");
    let heard = failing.heard();
    let ended = heard.last().unwrap();
    assert_eq!((ended.method.as_str(), ended.header("mcp-session-id")), ("DELETE", Some("made-up-session")));
    assert_eq!(ended.header("mcp-protocol-version"), Some("2025-11-25"));
}
