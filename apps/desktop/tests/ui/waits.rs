//! An agent's wait on its pull request, at the chat's end, with a real hub, the fake agent, and stand-ins for `gh` and
//! GitHub.

#![cfg(test)]

use gpui_kit::{SharedString, TestAppContext};
use hub::GithubPullRequestState::{Open, Passed};
use protocol::model::SessionStatus;

use crate::support::{Stack, Ui};

/// The wait's line is drawn from the hub's wait, and says what it waits on and where the pull request stands on GitHub,
/// as its checks change. Its Stop calls that wait off, and the line goes as the hub records it. The agent
/// hears that the person stopped it, in a turn of its own.
#[gpui_kit::test]
fn a_wait_s_line_says_where_its_pull_request_stands_and_stop_calls_it_off(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let github = stack.start_hub_reading_stand_in_github();
    github.pull_requests.set(7, Open);
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    // Its worker reads the address, so the hub knows which repository on GitHub its pull requests are of.
    ui.wait("the repository's address", |_| {
        stack.client().records_list().unwrap().repos.iter().any(|repo| repo.origin.url().is_some())
    });
    let chat = ui.active_chat();
    ui.input(r#"mcp:wait_for_pull_requests {"pull_requests":[7],"until":"merged"}"#);
    ui.press("enter");
    let waiting = || stack.client().session_get(&chat).unwrap().wait;
    ui.wait("the wait to start", |_| waiting().is_some());
    let wait = waiting().unwrap();
    let text = format!("wait-text/{}", wait.id);
    ui.wait("the line to say it", |ui| ui.shows_label(&text, "Waiting on #7: merge · 1 check running"));

    github.pull_requests.set(7, Passed);
    ui.input(r#"mcp:get_pull_requests {"pull_requests":[7]}"#);
    ui.press("enter");
    ui.wait("the line to follow", |ui| ui.shows_label(&text, "Waiting on #7: merge · 0 checks running"));
    // The agent's read moved the line, in a turn whose end is drawn above the line and moves it down as it comes. So Stop
    // is pressed once that's drawn, or the press can land where the button no longer is, and be lost.
    ui.wait("the read's turn to end", |ui| {
        ui.session_status() == Some(SessionStatus::Background) && ui.turn_drawn_to_its_end()
    });

    let stop = SharedString::from(format!("wait-stop/{}", wait.id));
    ui.click(stop.clone());

    // The hub's record first, then the line, so a wait that never ends says which didn't follow.
    ui.wait("the hub to end the wait", |_| waiting().is_none_or(|open| open.id != wait.id));
    ui.wait("the line to go with the hub's wait", |ui| !ui.has(stop.clone()));
    ui.wait("the agent to hear", |_| {
        let session = stack.client().session_get(&chat).unwrap();
        session.status == SessionStatus::Idle && session.background_tasks.is_empty()
    });
    ui.wait("it to say the person stopped it", |ui| ui.assistant_text().contains("The person stopped this wait."));
}
