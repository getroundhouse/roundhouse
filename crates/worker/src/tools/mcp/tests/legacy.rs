use super::current::{answered, asks, code, headers, hub_answers};
use super::*;
use crate::tools::mcp::legacy::VERSION as LEGACY;
use protocol::worker::{AskReply, PullRequestAnswer, WorkerAsk};
use serde_json::{Value, json};

fn said() -> Headers {
    Headers { version: Said::Value(LEGACY.into()), method: Said::Nothing, name: Said::Nothing }
}

fn body(method: &str, params: &Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"jsonrpc":"2.0","id":4,"method":method,"params":params})).unwrap()
}

#[tokio::test]
async fn initializes_without_modern_headers_and_offers_only_immediate_tools() {
    let (asks, _) = asks();
    let headers = Headers { version: Said::Nothing, method: Said::Nothing, name: Said::Nothing };
    let init = body("initialize", &json!({"protocolVersion":LEGACY,"capabilities":{}}));

    let initialized = answered(&asks, &headers, &init).await.to_json();
    let listed = answered(&asks, &said(), &body("tools/list", &json!({}))).await.to_json();

    assert_eq!(initialized["result"]["protocolVersion"], LEGACY);
    assert_eq!(initialized["result"]["capabilities"], json!({"tools":{}}));
    assert_eq!(initialized["result"]["serverInfo"]["name"], "roundhouse");
    assert!(initialized["result"].get("resultType").is_none());
    assert_eq!(listed["result"]["tools"], json!([pull_request::definition()]));
    assert!(listed["result"].get("resultType").is_none());
}

#[tokio::test]
async fn an_initialize_header_must_agree_with_its_body() {
    let (asks, _) = asks();
    let init = body("initialize", &json!({"protocolVersion":LEGACY}));

    let refused = answered(&asks, &headers("initialize", None), &init).await;

    assert_eq!(code(&refused), Code::HeaderMismatch);
}

#[tokio::test]
async fn a_legacy_call_uses_the_same_hub_ask_and_legacy_result() {
    let (asks, mut sent) = asks();
    let call = body("tools/call", &json!({"name":pull_request::NAME,"arguments":{}}));
    let headers = said();
    let calling = answered(&asks, &headers, &call);
    let hub = hub_answers(&asks, &mut sent, Ok(AskReply::PullRequest(PullRequestAnswer { pull_request: None })));

    let (answer, asked) = tokio::join!(calling, hub);

    assert_eq!(asked, WorkerAsk::PullRequest);
    assert_eq!(
        answer.to_json()["result"],
        json!({"content":[{"type":"text",
        "text":"This workspace's branch has no pull request on GitHub."}],"isError":false})
    );
}

#[tokio::test]
async fn a_legacy_client_cannot_start_or_manipulate_tasks() {
    let (asks, mut sent) = asks();
    for (method, params, expected) in [
        ("tools/call", json!({"name":wait::NAME,"arguments":{"until":"checks"}}), Code::InvalidParams),
        ("tasks/get", json!({"taskId":"w"}), Code::MethodNotFound),
        ("tasks/cancel", json!({"taskId":"w"}), Code::MethodNotFound),
    ] {
        let result = answered(&asks, &said(), &body(method, &params)).await;

        assert_eq!(code(&result), expected);
        assert!(sent.try_recv().is_err(), "refused before the hub is asked");
    }
}
