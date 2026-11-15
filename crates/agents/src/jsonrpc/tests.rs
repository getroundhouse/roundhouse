use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader, DuplexStream};

use super::{Dialect, Rpc, refused};
use crate::AgentError;

fn pipe_rpc(dialect: Dialect) -> (BufReader<DuplexStream>, Rpc) {
    let (writer, reader) = tokio::io::duplex(4096);
    let rpc = Rpc::new(writer, dialect, "TestPeer", None);
    (BufReader::new(reader), rpc)
}

#[tokio::test]
async fn jsonrpc2_frames_include_version_and_route_numeric_and_string_ids() {
    let (mut out, rpc) = pipe_rpc(Dialect::JsonRpc2);

    let (id, rx) = rpc.send_request("session/prompt", json!({ "sessionId": "s" })).await.unwrap();
    assert_eq!(id, 1);
    let mut line = String::new();
    out.read_line(&mut line).await.unwrap();
    let sent: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(sent, json!({ "jsonrpc": "2.0", "id": 1, "method": "session/prompt", "params": { "sessionId": "s" } }));

    // Server-initiated requests carry `method` even if their ID overlaps client request 1.
    assert!(
        !rpc.answered(&json!({ "jsonrpc": "2.0", "id": "1", "method": "session/request_permission", "params": {} }))
    );

    // Reply with string or numeric ID settles the pending client request.
    assert!(rpc.answered(&json!({ "jsonrpc": "2.0", "id": "1", "result": { "stopReason": "end_turn" } })));
    assert_eq!(rx.await.unwrap().unwrap(), json!({ "stopReason": "end_turn" }));

    rpc.reply(json!("srv-7"), json!({ "outcome": "cancelled" })).await.unwrap();
    line.clear();
    out.read_line(&mut line).await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap(),
        json!({ "jsonrpc": "2.0", "id": "srv-7", "result": { "outcome": "cancelled" } })
    );

    rpc.refuse(json!("srv-8"), "not here").await.unwrap();
    line.clear();
    out.read_line(&mut line).await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap(),
        json!({ "jsonrpc": "2.0", "id": "srv-8", "error": { "code": -32601, "message": "not here" } })
    );

    rpc.close().await;
}

#[tokio::test]
async fn bare_dialect_omits_jsonrpc_member_and_classifies_errors() {
    let (mut out, rpc) = pipe_rpc(Dialect::Bare);

    let (_, rx_unknown) = rpc.send_request("missing", json!({})).await.unwrap();
    let (_, rx_internal) = rpc.send_request("boom", json!({})).await.unwrap();
    let (_, rx_refused) = rpc.send_request("denied", json!({})).await.unwrap();

    let mut line = String::new();
    out.read_line(&mut line).await.unwrap();
    assert!(serde_json::from_str::<Value>(&line).unwrap().get("jsonrpc").is_none());

    rpc.answered(&json!({ "id": 1, "error": { "code": -32601, "message": "unknown" } }));
    rpc.answered(&json!({ "id": 2, "error": { "code": -32603, "message": "internal" } }));
    rpc.answered(&json!({ "id": 3, "error": { "code": -32000, "message": "auth required" } }));

    let e1 = rx_unknown.await.unwrap().unwrap_err();
    let e2 = rx_internal.await.unwrap().unwrap_err();
    let e3 = rx_refused.await.unwrap().unwrap_err();
    assert!(matches!(e1, AgentError::UnknownRequest(_)) && refused(&e1));
    assert!(matches!(e2, AgentError::Failed(_)) && !refused(&e2));
    assert!(matches!(e3, AgentError::Refused(_)) && refused(&e3));

    rpc.refuse(json!(9), "not here").await.unwrap();
    let mut last = String::new();
    for _ in 0..3 {
        last.clear();
        out.read_line(&mut last).await.unwrap();
    }
    assert_eq!(
        serde_json::from_str::<Value>(&last).unwrap(),
        json!({ "id": 9, "error": { "code": -32601, "message": "not here" } })
    );

    rpc.close().await;
}
