use hyper::StatusCode;
use serde_json::{Value, json};

use crate::conformance::{
    http::{self, Server, json_response},
    scenario::Scenario,
    scenarios::{ASKED, ECHO, PLAN, READY, SLOW, THOUGHT, WORD},
};

pub(super) async fn start(scenario: &'static Scenario) -> std::io::Result<Server> {
    Server::start(move |path, request, made| {
        if !path.ends_with("/responses") {
            return json_response(StatusCode::NOT_FOUND, &json!({"error":"not scripted"}));
        }
        let items = script(scenario.name, request, made);
        let id = format!("resp_scripted_{made}");
        let mut events = vec![("response.created", json!({"type":"response.created","response":{"id":id}}))];
        for (index, item) in items.iter().enumerate() {
            events.push((
                "response.output_item.added",
                json!({"type":"response.output_item.added","output_index":index,"item":item}),
            ));
            if item["type"] == "message" {
                events.push((
                    "response.output_text.delta",
                    json!({"type":"response.output_text.delta","item_id":item["id"],
                    "output_index":index,"content_index":0,"delta":item["content"][0]["text"]}),
                ));
            }
            events.push((
                "response.output_item.done",
                json!({"type":"response.output_item.done","output_index":index,"item":item}),
            ));
        }
        events.push((
            "response.completed",
            json!({"type":"response.completed","response":{"id":id,"status":"completed","output":items,
            "usage":{"input_tokens":100,"output_tokens":10,"total_tokens":110}}}),
        ));
        http::events(events, made)
    })
    .await
}

fn script(name: &str, request: &Value, made: u64) -> Vec<Value> {
    let input: Vec<_> = request["input"].as_array().into_iter().flatten().collect();
    let last_user =
        input.iter().rev().find(|item| item["role"] == "user").map_or_default(std::string::ToString::to_string);
    let output = input
        .iter()
        .rev()
        .find(|item| item["type"] == "custom_tool_call_output")
        .map(|item| item["output"].to_string());
    match name {
        "plain_turn" => vec![output.map_or_else(|| command(ECHO, made, false), |printed| message(&printed, made))],
        "cancel_command" if last_user.contains(READY) => vec![message("ready", made)],
        "cancel_command" => vec![command(SLOW, made, true)],
        "resume" if last_user.contains(ASKED) => {
            vec![message(if request.to_string().contains(WORD) { WORD } else { "I don't know" }, made)]
        }
        "resume" => vec![message("noted", made)],
        "thinking" => vec![
            json!({"type":"reasoning","id":format!("rs_{made}"),"summary":[{"type":"summary_text","text":THOUGHT}]}),
            message("Answer.", made),
        ],
        "plan_approved" if last_user.contains("Implement the approved plan") => vec![message("approved", made)],
        "plan_approved" => vec![message(&format!("<proposed_plan>\n{PLAN}\n</proposed_plan>"), made)],
        _ => vec![message("The scenario has no script", made)],
    }
}

fn message(text: &str, made: u64) -> Value {
    json!({"type":"message","id":format!("msg_{made}"),"role":"assistant","status":"completed",
        "content":[{"type":"output_text","text":text,"annotations":[]}]})
}

fn command(command: &str, made: u64, wait: bool) -> Value {
    let options = json!({"cmd":command,"login":false,"yield_time_ms":if wait {1} else {1000}});
    let mut code = format!("const result = await tools.exec_command({options}); text(result);");
    if wait {
        code.push_str(" if (result.session_id) text(await tools.write_stdin({session_id:result.session_id,chars:'',yield_time_ms:30000}));");
    }
    json!({"type":"custom_tool_call","id":format!("fc_{made}"),"call_id":format!("call_{made}"),
        "name":"exec","namespace":"functions","input":code})
}
