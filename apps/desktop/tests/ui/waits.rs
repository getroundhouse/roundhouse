//! An agent's wait on its pull request, at the chat's end, with a real hub, the fake agent, and a stand-in `gh`.

#![cfg(test)]

use gpui_kit::{SharedString, TestAppContext};
use protocol::model::SessionStatus;
use serde_json::{Value, json};

use crate::support::{Stack, Ui};

/// The same PR with its required check running, then passed, while it still waits to merge.
fn observation(passed: bool) -> String {
    let mut response: Value =
        serde_json::from_str(include_str!("../../../../crates/worker/tests/fixtures/remote_pull_requests.json"))
            .unwrap();
    response["data"]["repository"]["pullRequest"]["statusCheckRollup"]["contexts"]["nodes"] = json!([{
        "__typename":"StatusContext", "context":"checks", "targetUrl":"https://example.com/check/1",
        "state":if passed {"SUCCESS"} else {"PENDING"}, "createdAt":"2026-01-01T00:00:00Z",
        "isRequired":true, "commit":{"oid":"remote-head"}
    }]);
    response.to_string()
}

/// The wait's line is drawn from the hub's wait, and says what it waits on and where the pull request stands on GitHub,
/// as its checks change. Its Stop calls that wait off, and the line goes as the hub records it. The agent
/// hears that the person stopped it, in a turn of its own.
#[gpui_kit::test]
fn a_wait_s_line_says_where_its_pull_request_stands_and_stop_calls_it_off(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let gh = stack.start_hub_with_stand_in_gh();
    gh.report_remote(&observation(false));
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    ui.input(r#"mcp:wait_for_pull_requests {"pull_requests":[7],"until":"merged"}"#);
    ui.press("enter");
    let waiting = || stack.client().session_get(&chat).unwrap().wait;
    ui.wait("the wait to start", |_| waiting().is_some());
    let wait = waiting().unwrap();
    let text = format!("wait-text/{}", wait.id);
    ui.wait("the line to say it", |ui| ui.shows_label(&text, "Waiting on #7: merge · 1 check running"));

    gh.report_remote(&observation(true));
    ui.input(r#"mcp:get_pull_requests {"pull_requests":[7]}"#);
    ui.press("enter");
    ui.wait("the line to follow", |ui| ui.shows_label(&text, "Waiting on #7: merge · 0 checks running"));

    let stop = SharedString::from(format!("wait-stop/{}", wait.id));
    ui.click(stop.clone());

    ui.wait("the line to go with the hub's wait", |ui| !ui.has(stop.clone()));
    ui.wait("the agent to hear", |_| {
        let session = stack.client().session_get(&chat).unwrap();
        session.status == SessionStatus::Idle && session.background_tasks.is_empty()
    });
    ui.wait("it to say the person stopped it", |ui| ui.assistant_text().contains("The person stopped this wait."));
}
