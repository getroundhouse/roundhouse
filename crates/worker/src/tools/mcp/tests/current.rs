use crate::tools::pull_requests;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{WaitEnd, WaitUntil};
use protocol::worker::{
    AskAnswer, AskOutcome, AskReply, CancelWait, PullRequestAnswer, ReadWait, StartWait, WaitAnswer, WorkerAsk,
    WorkerFrame,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::*;
use crate::tools::wait::tests::{ended, waiting};

/// Asks that go nowhere, for requests that don't ask the hub anything.
pub(super) fn asks() -> (Asks, mpsc::Receiver<WorkerFrame>) {
    let (frames, sent) = mpsc::channel(8);
    (Asks::new(frames), sent)
}

/// Headers that agree with a request for `method`, as the CLI sends them.
pub(super) fn headers(method: &str, name: Option<&str>) -> Headers {
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

/// A request from a client that offers to take a task, as the Claude CLI's do, naming the agent's call as it does.
fn offering_tasks(method: &str, params: &Value) -> Vec<u8> {
    let mut body: Value = serde_json::from_slice(&request(method, params)).unwrap();
    body["params"]["_meta"][CAPABILITIES_KEY] = json!({ "extensions": { TASKS: {} } });
    body["params"]["_meta"]["claudecode/toolUseId"] = json!("toolu_1");
    serde_json::to_vec(&body).unwrap()
}

/// Answers the next ask as the hub would, with `reply`, and says what was asked.
pub(super) async fn hub_answers(
    asks: &Asks,
    sent: &mut mpsc::Receiver<WorkerFrame>,
    reply: Result<AskReply, ApiError>,
) -> WorkerAsk {
    // Within a time, so a request answered without asking fails the test rather than holding it.
    let asked = tokio::time::timeout(std::time::Duration::from_secs(5), sent.recv()).await;
    let Ok(Some(WorkerFrame::Ask { id, session_id, ask })) = asked else { panic!("the hub wasn't asked") };
    assert_eq!(session_id.as_str(), "s");
    asks.answer(AskAnswer { answer: id, outcome: reply.into() });
    ask
}

pub(super) async fn answered(asks: &Asks, headers: &Headers, body: &[u8]) -> Answer {
    match answer(asks, Build::UNSTAMPED, &"s".into(), headers, body).await {
        Reply::Answer(answer) => answer,
        Reply::Accepted => panic!("a request is answered"),
    }
}

pub(super) fn code(answer: &Answer) -> Code {
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
    assert_eq!(json["result"]["capabilities"]["extensions"], json!({ TASKS: {} }), "it says it can answer with a task");
    assert_eq!(json["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"], "roundhouse");
}

#[tokio::test]
async fn the_tools_listed_are_the_ones_it_answers() {
    let (asks, _) = asks();

    let answer = answered(&asks, &headers("tools/list", None), &request("tools/list", &json!({}))).await;

    let tools = answer.to_json()["result"]["tools"].clone();
    assert_eq!(tools.as_array().map(Vec::len), Some(2));
    for (tool, name) in [(&tools[0], pull_requests::READ), (&tools[1], pull_requests::WAIT)] {
        assert_eq!(tool["name"], name);
        assert!(Tool::named(name).is_some(), "{name} is answered");
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
    }
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
    assert_eq!(data["supported"], json!([VERSION, crate::tools::mcp::legacy::VERSION]));
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

    let taken = answer(&asks, Build::UNSTAMPED, &"s".into(), &said, notification).await;
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
        // Within a time, so a request answered without asking fails the test rather than holding it.
        let asked = tokio::time::timeout(std::time::Duration::from_secs(5), sent.recv()).await;
        let Ok(Some(WorkerFrame::Ask { id, session_id, ask })) = asked else { panic!("the hub wasn't asked") };
        assert_eq!((session_id.as_str(), ask), ("s", WorkerAsk::PullRequest));
        let outcome = AskOutcome::Response(AskReply::PullRequest(PullRequestAnswer { pull_request: None }));
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

#[tokio::test]
async fn a_wait_is_started_for_the_call_that_asked_and_answered_as_a_task_whose_id_is_the_waits() {
    let (asks, mut sent) = asks();
    let call = offering_tasks("tools/call", &json!({ "name": wait::NAME, "arguments": { "until": "checks" } }));
    let said = headers("tools/call", Some(wait::NAME));
    let calling = answered(&asks, &said, &call);
    let hub = hub_answers(&asks, &mut sent, Ok(AskReply::Wait(WaitAnswer { wait: waiting(), pull_request: None })));

    let (answer, asked) = tokio::join!(calling, hub);

    assert_eq!(asked, WorkerAsk::StartWait(StartWait { until: WaitUntil::Checks, call: Some("toolu_1".into()) }));
    let result = &answer.to_json()["result"];
    assert_eq!((&result["resultType"], &result["taskId"]), (&json!("task"), &json!("w1")));
    assert_eq!(result["status"], "working");
}

#[tokio::test]
async fn a_wait_that_ended_as_it_started_or_couldnt_start_is_answered_at_once() {
    let (asks, mut sent) = asks();
    let call = offering_tasks("tools/call", &json!({ "name": wait::NAME }));
    let said = headers("tools/call", Some(wait::NAME));
    let no_pr = ApiError::new(ErrorCode::NotFound, "this workspace's branch has no pull request to wait on");

    let merged = Ok(AskReply::Wait(WaitAnswer { wait: ended(WaitEnd::Merged), pull_request: None }));
    let (done, _) = tokio::join!(answered(&asks, &said, &call), hub_answers(&asks, &mut sent, merged));
    let (refused, _) = tokio::join!(answered(&asks, &said, &call), hub_answers(&asks, &mut sent, Err(no_pr)));

    let done = &done.to_json()["result"];
    assert_eq!((&done["resultType"], &done["isError"]), (&json!("complete"), &json!(false)));
    assert!(done["content"][0]["text"].as_str().unwrap().contains("has ended: it's merged."), "{done}");
    let refused = &refused.to_json()["result"];
    assert_eq!((&refused["resultType"], &refused["isError"]), (&json!("complete"), &json!(true)));
    assert!(refused["content"][0]["text"].as_str().unwrap().contains("no pull request to wait on"), "{refused}");
}

/// A wait can only be answered as a task, so the client has to offer to take one, as MCP's tasks extension says.
#[tokio::test]
async fn a_client_that_doesnt_take_tasks_is_told_a_wait_needs_them() {
    let (asks, _) = asks();
    let call = request("tools/call", &json!({ "name": wait::NAME }));
    let get = request("tasks/get", &json!({ "taskId": "w1" }));

    let started = answered(&asks, &headers("tools/call", Some(wait::NAME)), &call).await;
    let asked = answered(&asks, &headers("tasks/get", Some("w1")), &get).await;

    for refused in [started, asked] {
        assert_eq!(code(&refused), Code::MissingCapability);
        let data = &refused.to_json()["error"]["data"];
        assert_eq!(data["requiredCapabilities"]["extensions"], json!({ TASKS: {} }));
    }
}

#[tokio::test]
async fn asking_after_a_task_or_cancelling_it_asks_the_hub_about_the_wait_both_header_and_body_name() {
    let (asks, mut sent) = asks();
    let (get, cancel) = (
        offering_tasks("tasks/get", &json!({ "taskId": "w1" })),
        offering_tasks("tasks/cancel", &json!({ "taskId": "w1" })),
    );
    let (getting, cancelling) = (headers("tasks/get", Some("w1")), headers("tasks/cancel", Some("w1")));
    let merged = Ok(AskReply::Wait(WaitAnswer { wait: ended(WaitEnd::Merged), pull_request: None }));
    let cancelled = Ok(AskReply::Wait(WaitAnswer { wait: ended(WaitEnd::Cancelled), pull_request: None }));

    let (got, get_asked) = tokio::join!(answered(&asks, &getting, &get), hub_answers(&asks, &mut sent, merged));
    let (ended, cancel_asked) =
        tokio::join!(answered(&asks, &cancelling, &cancel), hub_answers(&asks, &mut sent, cancelled));
    let elsewhere = answered(&asks, &headers("tasks/get", Some("w2")), &get).await;

    assert_eq!(get_asked, WorkerAsk::Wait(ReadWait { wait: "w1".into() }));
    assert_eq!(cancel_asked, WorkerAsk::CancelWait(CancelWait { wait: "w1".into() }));
    let got = &got.to_json()["result"];
    assert_eq!((&got["resultType"], &got["status"]), (&json!("complete"), &json!("completed")));
    assert_eq!(ended.to_json()["result"], json!({ "resultType": "complete" }));
    assert_eq!(code(&elsewhere), Code::HeaderMismatch, "the header names another task");
}

#[tokio::test]
async fn a_task_that_isnt_the_chats_is_unknown_and_a_hub_that_cant_say_is_mcps_failure() {
    let (asks, mut sent) = asks();
    let get = offering_tasks("tasks/get", &json!({ "taskId": "w9" }));
    let said = headers("tasks/get", Some("w9"));
    let not_found = Err(ApiError::new(ErrorCode::NotFound, "this chat has no wait w9"));
    let unavailable = Err(ApiError::new(ErrorCode::Unavailable, "the worker lost its hub"));

    let (unknown, _) = tokio::join!(answered(&asks, &said, &get), hub_answers(&asks, &mut sent, not_found));
    let (failed, _) = tokio::join!(answered(&asks, &said, &get), hub_answers(&asks, &mut sent, unavailable));

    assert_eq!(code(&unknown), Code::InvalidParams);
    assert_eq!(code(&failed), Code::Internal, "the CLI asks again after an error");
}

/// As the Claude CLI cancels the task of a call interrupted just as it was answered, so the wait doesn't run on.
#[tokio::test]
async fn a_task_cancelled_without_its_name_in_a_header_is_still_cancelled() {
    let (asks, mut sent) = asks();
    let cancel = offering_tasks("tasks/cancel", &json!({ "taskId": "w1" }));
    let unnamed = headers("tasks/cancel", None);
    let cancelled = Ok(AskReply::Wait(WaitAnswer { wait: ended(WaitEnd::Cancelled), pull_request: None }));

    let (answer, asked) = tokio::join!(answered(&asks, &unnamed, &cancel), hub_answers(&asks, &mut sent, cancelled));

    assert_eq!(asked, WorkerAsk::CancelWait(CancelWait { wait: "w1".into() }));
    assert_eq!(answer.to_json()["result"], json!({ "resultType": "complete" }));
}

/// A wait never asks for input, so an update has nothing to do, once it's known to be the chat's task.
#[tokio::test]
async fn an_update_to_a_task_is_taken_and_an_offer_of_nothing_is_no_offer() {
    let (asks, mut sent) = asks();
    let update = offering_tasks("tasks/update", &json!({ "taskId": "w1", "inputResponses": {} }));
    let said = headers("tasks/update", Some("w1"));
    let waits = Ok(AskReply::Wait(WaitAnswer { wait: waiting(), pull_request: None }));
    let mut no_offer: Value = serde_json::from_slice(&offering_tasks("tasks/get", &json!({ "taskId": "w1" }))).unwrap();
    no_offer["params"]["_meta"][CAPABILITIES_KEY]["extensions"][TASKS] = Value::Null;

    let (updated, asked) = tokio::join!(answered(&asks, &said, &update), hub_answers(&asks, &mut sent, waits));
    let refused = answered(&asks, &headers("tasks/get", Some("w1")), &serde_json::to_vec(&no_offer).unwrap()).await;

    assert_eq!(asked, WorkerAsk::Wait(ReadWait { wait: "w1".into() }));
    assert_eq!(updated.to_json()["result"], json!({ "resultType": "complete" }));
    assert_eq!(code(&refused), Code::MissingCapability);
}
