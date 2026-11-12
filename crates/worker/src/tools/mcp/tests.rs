use protocol::worker::{AskAnswer, AskOutcome, AskReply, WorkerAsk, WorkerFrame};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::*;

/// Asks that go nowhere, for requests that don't ask the hub anything.
fn asks() -> (Asks, mpsc::Receiver<WorkerFrame>) {
    let (frames, sent) = mpsc::channel(8);
    (Asks::new(frames), sent)
}

/// Headers that agree with a request for `method`, as the CLI sends them.
fn headers(method: &str, name: Option<&str>) -> Headers {
    Headers {
        version: Said::Value(VERSION.into()),
        method: Said::Value(method.into()),
        name: name.map_or(Said::Nothing, |name| Said::Value(name.into())),
    }
}

fn request(method: &str, params: &Value) -> Vec<u8> {
    let mut params = params.clone();
    params["_meta"] = json!({ VERSION_KEY: VERSION, "io.modelcontextprotocol/clientCapabilities": {} });
    serde_json::to_vec(&json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params })).unwrap()
}

async fn answered(asks: &Asks, headers: &Headers, body: &[u8]) -> Answer {
    match answer(asks, &"s".into(), headers, body).await {
        Reply::Answer(answer) => answer,
        Reply::Accepted => panic!("a request is answered"),
    }
}

fn code(answer: &Answer) -> Code {
    answer.outcome.as_ref().map(|_| ()).map_err(|e| e.code).expect_err("it was refused")
}

#[tokio::test]
async fn discovering_says_which_version_it_speaks_and_that_it_has_tools() {
    let (asks, _) = asks();

    let answer = answered(&asks, &headers("server/discover", None), &request("server/discover", &json!({}))).await;

    let json = answer.to_json();
    assert_eq!(json["id"], 7);
    assert_eq!(json["result"]["supportedVersions"], json!([VERSION]));
    assert_eq!(json["result"]["capabilities"]["tools"], json!({}));
    assert_eq!(json["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"], "roundhouse");
}

#[tokio::test]
async fn the_tools_listed_are_the_ones_it_answers() {
    let (asks, _) = asks();

    let answer = answered(&asks, &headers("tools/list", None), &request("tools/list", &json!({}))).await;

    let tools = answer.to_json()["result"]["tools"].clone();
    assert_eq!(tools.as_array().map(Vec::len), Some(1));
    assert_eq!(tools[0]["name"], pull_request::NAME);
    assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
}

#[tokio::test]
async fn a_version_it_doesnt_speak_is_refused_naming_the_one_it_does() {
    let (asks, _) = asks();
    let mut body: Value = serde_json::from_slice(&request("tools/list", &json!({}))).unwrap();
    body["params"]["_meta"][VERSION_KEY] = json!("2025-11-25");
    let mut said = headers("tools/list", None);
    said.version = Said::Value("2025-11-25".into());

    let answer = answered(&asks, &said, &serde_json::to_vec(&body).unwrap()).await;

    assert_eq!(code(&answer), Code::UnsupportedVersion);
    let data = &answer.to_json()["error"]["data"];
    assert_eq!(data["supported"], json!([VERSION]));
    assert_eq!(data["requested"], "2025-11-25");
}

#[tokio::test]
async fn the_handshake_of_the_versions_before_is_refused_naming_the_one_it_speaks() {
    let (asks, _) = asks();
    let body =
        json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-11-25" } });
    let said = Headers { version: Said::Nothing, method: Said::Nothing, name: Said::Nothing };

    let answer = answered(&asks, &said, &serde_json::to_vec(&body).unwrap()).await;

    assert_eq!(code(&answer), Code::UnsupportedVersion);
    assert_eq!(answer.to_json()["error"]["data"]["requested"], "2025-11-25");
}

#[tokio::test]
async fn headers_that_dont_say_what_the_body_does_are_refused() {
    let (asks, _) = asks();
    let call = request("tools/call", &json!({ "name": pull_request::NAME }));
    let cases = [
        ("no version", Headers { version: Said::Nothing, ..headers("tools/call", Some(pull_request::NAME)) }),
        ("no method", Headers { method: Said::Nothing, ..headers("tools/call", Some(pull_request::NAME)) }),
        ("another method", headers("tools/list", Some(pull_request::NAME))),
        ("no name", headers("tools/call", None)),
        ("another name", headers("tools/call", Some("something_else"))),
        ("an unreadable name", Headers { name: Said::Unreadable, ..headers("tools/call", None) }),
    ];

    for (case, said) in cases {
        assert_eq!(code(&answered(&asks, &said, &call).await), Code::HeaderMismatch, "{case}");
    }
}

#[tokio::test]
async fn a_method_or_tool_it_doesnt_have_is_refused() {
    let (asks, _) = asks();

    let method = answered(&asks, &headers("prompts/list", None), &request("prompts/list", &json!({}))).await;
    let tool = answered(
        &asks,
        &headers("tools/call", Some("make_coffee")),
        &request("tools/call", &json!({ "name": "make_coffee" })),
    )
    .await;

    assert_eq!(code(&method), Code::MethodNotFound);
    assert_eq!(code(&tool), Code::InvalidParams);
}

#[tokio::test]
async fn a_notification_is_taken_and_what_isnt_json_rpc_is_refused() {
    let (asks, _) = asks();
    let said = headers("tools/list", None);
    let notification = br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;

    let taken = answer(&asks, &"s".into(), &said, notification).await;
    let garbled = answered(&asks, &said, b"{not json").await;
    let refused = [
        (&br#"{"jsonrpc":"2.0","id":3}"#[..], json!(3), "no method"),
        (br#"{"id":3,"method":"tools/list"}"#, json!(3), "not JSON-RPC 2.0"),
        (br#"{"jsonrpc":"2.0","id":null,"method":"tools/list"}"#, Value::Null, "an ID that's null"),
        (br#"{"jsonrpc":"2.0","id":3,"method":5}"#, json!(3), "a method that isn't text"),
        (br#"[{"jsonrpc":"2.0","id":3,"method":"tools/list"}]"#, Value::Null, "a batch"),
    ];

    assert!(matches!(taken, Reply::Accepted));
    assert_eq!(code(&garbled), Code::Parse);
    assert_eq!(garbled.to_json()["id"], Value::Null);
    for (body, id, case) in refused {
        let answer = answered(&asks, &said, body).await;
        assert_eq!(code(&answer), Code::InvalidRequest, "{case}");
        assert_eq!(answer.to_json()["id"], id, "{case}");
    }
}

#[tokio::test]
async fn every_result_says_its_complete_and_lists_say_how_long_to_keep_them() {
    let (asks, _) = asks();

    for method in ["server/discover", "ping", "tools/list"] {
        let result = answered(&asks, &headers(method, None), &request(method, &json!({}))).await.to_json();

        assert_eq!(result["result"]["resultType"], "complete", "{method}");
        if method != "ping" {
            assert_eq!((&result["result"]["ttlMs"], &result["result"]["cacheScope"]), (&json!(0), &json!("public")));
        }
    }
}

#[tokio::test]
async fn a_call_asks_the_hub_for_the_chat_that_called_and_answers_with_what_it_says() {
    let (asks, mut sent) = asks();
    let call = request("tools/call", &json!({ "name": pull_request::NAME, "arguments": {} }));
    let said = headers("tools/call", Some(pull_request::NAME));
    let calling = answered(&asks, &said, &call);
    let hub = async {
        let Some(WorkerFrame::Ask { id, session_id, ask }) = sent.recv().await else { panic!("the hub wasn't asked") };
        assert_eq!((session_id.as_str(), ask), ("s", WorkerAsk::PullRequest));
        let outcome = AskOutcome::Response(AskReply::PullRequest { pull_request: None });
        asks.answer(AskAnswer { answer: id, outcome });
    };

    let (answer, ()) = tokio::join!(calling, hub);

    let result = &answer.to_json()["result"];
    assert_eq!(result["isError"], false);
    assert_eq!(result["content"][0]["text"], "This workspace's branch has no pull request on GitHub.");
}

#[tokio::test]
async fn a_call_the_hub_cant_answer_is_the_tools_failure_not_mcps() {
    let (asks, sent) = asks();
    drop(sent);
    let call = request("tools/call", &json!({ "name": pull_request::NAME }));

    let answer = answered(&asks, &headers("tools/call", Some(pull_request::NAME)), &call).await;

    let result = &answer.to_json()["result"];
    assert_eq!(result["isError"], true);
    assert_eq!(result["content"][0]["type"], "text", "{result}");
}
