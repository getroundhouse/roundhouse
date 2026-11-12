//! An agent's wait on its pull request, at the chat's end, with a real hub, the fake agent, and a stand-in `gh`.

#![cfg(test)]

use gpui_kit::{SharedString, TestAppContext};
use protocol::model::SessionStatus;

use crate::support::{Stack, Ui};

/// A pull request that's open and needs a review, or with `approved`, is ready to merge.
fn open(approved: bool) -> String {
    let (review, merge) = if approved { ("APPROVED", "CLEAN") } else { ("REVIEW_REQUIRED", "BLOCKED") };
    format!(
        r#"{{"number":7,"url":"https://example.com/pull/7","title":"T","state":"OPEN","isDraft":false,"reviewDecision":"{review}","mergeStateStatus":"{merge}"}}"#
    )
}

/// The wait's line is drawn from the hub's wait, and says what it waits on and where the pull request stands on GitHub,
/// in the bar's words, as they change. Its Stop calls that wait off, and the line goes as the hub records it. The agent
/// hears that the person stopped it, in a turn of its own.
#[gpui_kit::test]
fn a_wait_s_line_says_where_its_pull_request_stands_and_stop_calls_it_off(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let gh = stack.start_hub_with_stand_in_gh();
    gh.report(&open(false));
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    ui.input("mcp:wait_for_pull_request");
    ui.press("enter");
    let waiting = || stack.client().session_get(&chat).unwrap().wait;
    ui.wait("the wait to start", |_| waiting().is_some());
    let wait = waiting().unwrap();
    let text = format!("wait-text/{}", wait.id);
    ui.wait("the line to say it", |ui| ui.shows_label(&text, "Waiting on #7 \u{b7} Review required"));

    gh.report(&open(true));
    stack.client().workspace_pull_request(&wait.workspace_id).unwrap();
    ui.wait("the line to follow", |ui| ui.shows_label(&text, "Waiting on #7 \u{b7} Ready to merge"));

    let stop = SharedString::from(format!("wait-stop/{}", wait.id));
    ui.click(stop.clone());

    ui.wait("the line to go with the hub's wait", |ui| !ui.has(stop.clone()));
    ui.wait("the agent to hear", |_| {
        let session = stack.client().session_get(&chat).unwrap();
        session.status == SessionStatus::Idle && session.background_tasks.is_empty()
    });
    ui.wait("it to say the person stopped it", |ui| ui.assistant_text().contains("The person stopped this wait."));
}
