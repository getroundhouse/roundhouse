use hyper::StatusCode;
use serde_json::{Value, json};

use crate::conformance::{
    http::{self, Server, json_response},
    scenario::Scenario,
    scenarios::{ASKED, ECHO, PLAN, READY, SLOW, THOUGHT, WORD},
};

pub(super) async fn start(scenario: &'static Scenario) -> std::io::Result<Server> {
    Server::start(move |path, request, made| {
        if !path.contains(":streamGenerateContent") {
            return json_response(StatusCode::NOT_FOUND, &json!({ "error": "not scripted" }));
        }
        if path.contains("flash-lite") {
            return http::data_events(vec![text_chunk("Scenario trial")], made);
        }
        http::data_events(script(scenario.name, request), made)
    })
    .await
}

fn script(name: &str, request: &Value) -> Vec<Value> {
    let contents: Vec<&Value> = request["contents"].as_array().into_iter().flatten().collect();
    let user_texts: Vec<String> = contents
        .iter()
        .filter(|c| c["role"] == "user")
        .map(|c| {
            c["parts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|text| !text.is_empty())
        .collect();
    let last_user = user_texts
        .iter()
        .rev()
        .find(|text| text.contains("<USER_REQUEST>"))
        .or_else(|| user_texts.last())
        .cloned()
        .unwrap_or_default();
    let fn_output = contents.last().and_then(|last| {
        last["parts"].as_array()?.iter().find_map(|part| {
            let resp = part.get("functionResponse")?.get("response")?;
            resp.get("output").and_then(Value::as_str).map(str::to_owned).or_else(|| Some(resp.to_string()))
        })
    });
    match name {
        "plain_turn" => match fn_output {
            Some(printed) => vec![text_chunk(&printed)],
            None => vec![command_chunk(ECHO, 5000)],
        },
        "cancel_command" if last_user.contains(READY) => vec![text_chunk("ready")],
        "cancel_command" => vec![command_chunk(SLOW, 10_000)],
        "resume" if last_user.contains(ASKED) => {
            let said = if request.to_string().contains(WORD) { WORD } else { "I don't know" };
            vec![text_chunk(said)]
        }
        "resume" => vec![text_chunk("noted")],
        "thinking" => vec![
            json!({
                "candidates": [{ "content": { "role": "model", "parts": [{ "text": THOUGHT, "thought": true }] } }]
            }),
            text_chunk("Answer."),
        ],
        "plan_approved" if last_user.contains("Implement the approved plan") => vec![text_chunk("approved")],
        "plan_approved" => vec![text_chunk(PLAN)],
        _ => vec![text_chunk("The scenario has no script")],
    }
}

fn text_chunk(text: &str) -> Value {
    json!({
        "candidates": [{
            "content": { "role": "model", "parts": [{ "text": text }] },
            "finishReason": "STOP"
        }],
        "usageMetadata": {
            "promptTokenCount": 100,
            "candidatesTokenCount": 10,
            "totalTokenCount": 110
        }
    })
}

fn command_chunk(command: &str, wait_ms: u64) -> Value {
    json!({
        "candidates": [{
            "content": {
                "role": "model",
                "parts": [{
                    "functionCall": {
                        "name": "run_command",
                        "args": {
                            "CommandLine": command,
                            "toolSummary": "Run scenario command",
                            "WaitMsBeforeAsync": wait_ms
                        }
                    }
                }]
            },
            "finishReason": "STOP"
        }],
        "usageMetadata": {
            "promptTokenCount": 100,
            "candidatesTokenCount": 15,
            "totalTokenCount": 115
        }
    })
}
