//! Losing the hub, and what's waiting when the app comes back.

#![cfg(test)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use client_core::Client;
use desktop::app::Connection;
use gpui_kit::{AppContext as _, ElementId, SharedString, TestAppContext};
use protocol::model::SessionStatus;

use crate::support::{Diff, Stack, TIMEOUT, Ui};

#[gpui_kit::test]
fn losing_the_hub_starts_a_new_one_and_picks_up_where_it_left_off(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("say:before");
    ui.press("enter");
    ui.wait_idle_with("the first turn", |text| text.contains("before"));
    ui.input("say:after");
    let (pane, transcript) = (ui.pane().unwrap(), ui.transcript().unwrap());

    let pid = stack.crash_hub();

    ui.wait_for_new_hub(&stack, pid);
    // The same workspace view, with what was typed and the history it had, carrying on on the new hub.
    assert_eq!(ui.pane().unwrap().entity_id(), pane.entity_id());
    assert_eq!(ui.transcript().unwrap().entity_id(), transcript.entity_id());
    assert_eq!(ui.composer_text(), "say:after");
    ui.wait("the history", |ui| ui.assistant_text().contains("before"));
    ui.press("enter");
    ui.wait_idle_with("the next turn", |text| text.contains("after"));
}

/// What's typed in a composer, or in a comment on the diff, is only in the app until it's sent.
#[gpui_kit::test]
fn whats_typed_is_still_there_after_the_hub_restarts(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let id = ui.new_workspace();
    let first = ui.active_chat();
    let second = ui.new_chat();
    ui.input("say:from the second chat");
    ui.show_chat(&first);
    ui.frame();
    ui.input("say:from the first chat");
    std::fs::write(ui.worktree(&id).join("a.txt"), "one\ntwo\n").unwrap();
    let diff = ui.open_diff();
    ui.open_file(&diff, "a.txt");
    ui.start_comment(&diff, "a.txt", 2);
    ui.input("Why");

    let pid = stack.client().hub_status().unwrap().pid;
    client_core::stop_hub(&stack.hub.paths.socket).unwrap();
    ui.wait_for_new_hub(&stack, pid);

    // The comment still has the keys, so the rest of it doesn't go to the agent, and saves in the new hub.
    ui.input(" two?");
    ui.press("enter");
    ui.wait("the comment to be saved", |_| {
        stack.client().comment_list(&id).unwrap().iter().map(|c| c.text.clone()).collect::<Vec<_>>() == ["Why two?"]
    });
    assert_eq!(ui.composer_text(), "say:from the first chat");

    // Each chat keeps its draft, and sends it on the new hub.
    ui.show_chat(&second);
    ui.frame();
    assert_eq!(ui.composer_text(), "say:from the second chat");
    ui.press("enter");
    ui.wait_idle_with("the second chat's reply", |text| text.contains("from the second chat"));
    ui.show_chat(&first);
    ui.frame();
    ui.press("enter");
    ui.wait_idle_with("the first chat's reply", |text| text.contains("from the first chat"));
}

/// After a reconnect, a pane reads the worktree again once it's on screen, so the hub doesn't start a worker for
/// every pane at once.
#[gpui_kit::test]
fn after_a_reconnect_panes_catch_up_when_shown_and_an_archived_ones_goes(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let daemon = stack.without_a_daemon();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let archived = ui.new_workspace();
    let hidden = ui.new_workspace();
    ui.input("say:still here");
    let hidden_diff = open_empty_diff(&mut ui);
    let shown = ui.new_workspace();
    let shown_diff = open_empty_diff(&mut ui);
    lose_the_hub(&mut ui, &stack);
    // A kept pane does nothing until there's a connection for it, even when a shortcut reaches the window.
    ui.cx.update_window(ui.handle, |_, window, cx| window.blur(cx)).unwrap();
    ui.frame();
    ui.press("cmd-shift-d");
    assert!(ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.changes_open));
    let client = Client::connect_or_start(&stack.hub.paths, &daemon, "test").unwrap();
    client.workspace_archive(&archived, false).unwrap();
    for id in [&hidden, &shown] {
        std::fs::write(ui.worktree(id).join("a.txt"), "one\nchanged while away\n").unwrap();
    }

    ui.click("try-hub-again");

    ui.wait("the pane on screen to catch up", |ui| {
        matches!(ui.view().connection, Connection::Ready(_)) && shows_changes(ui, &shown_diff)
    });
    assert!(!ui.view().panes.contains_key(&archived));
    assert!(!shows_changes(&ui, &hidden_diff), "it waits until it's shown");
    ui.click(SharedString::from(format!("workspace-{hidden}")));
    ui.wait("the other pane to catch up once shown", |ui| shows_changes(ui, &hidden_diff));
    assert_eq!(ui.composer_text(), "say:still here");
}

/// A pane still behind from one reconnect, and shown while the hub's gone again, doesn't try the lost
/// connection. It catches up on the next.
#[gpui_kit::test]
fn a_pane_shown_while_the_hub_is_gone_catches_up_once_it_is_back(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let daemon = stack.without_a_daemon();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let behind = ui.new_workspace();
    let diff = open_empty_diff(&mut ui);
    ui.new_workspace();
    lose_the_hub(&mut ui, &stack);
    Client::connect_or_start(&stack.hub.paths, &daemon, "test").unwrap();
    ui.click("try-hub-again");
    ui.wait("the hub to come back", |ui| matches!(ui.view().connection, Connection::Ready(_)));
    lose_the_hub(&mut ui, &stack);
    std::fs::write(ui.worktree(&behind).join("a.txt"), "one\nchanged while away\n").unwrap();

    ui.click(SharedString::from(format!("workspace-{behind}")));
    ui.frame();

    assert_eq!(ui.view().selected.as_ref(), Some(&behind));
    assert_eq!(ui.view().error.as_deref(), None);
    Client::connect_or_start(&stack.hub.paths, &daemon, "test").unwrap();
    ui.click("try-hub-again");
    ui.wait("it to catch up", |ui| matches!(ui.view().connection, Connection::Ready(_)) && shows_changes(ui, &diff));
    assert_eq!(ui.view().error.as_deref(), None);
}

/// Stops the hub, and waits for the app to find it can't start another.
fn lose_the_hub(ui: &mut Ui<'_>, stack: &Stack) {
    client_core::stop_hub(&stack.hub.paths.socket).unwrap();
    ui.wait("the app to find no hub", |ui| matches!(ui.view().connection, Connection::Failed(_)));
}

/// Opens the selected workspace's diff, which has nothing in it yet.
fn open_empty_diff(ui: &mut Ui<'_>) -> Diff {
    let diff = ui.open_diff();
    assert!(!shows_changes(ui, &diff));
    diff
}

fn shows_changes(ui: &Ui<'_>, diff: &Diff) -> bool {
    ui.cx.read_entity(&diff.review, |r, _| r.files.as_ref().is_some_and(|files| !files.is_empty()))
}

#[gpui_kit::test]
fn a_failed_connection_can_be_tried_again(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    let daemon = stack.without_a_daemon();
    let mut ui = Ui::open_without_waiting(cx, &stack);
    ui.wait("the failure", |ui| matches!(ui.view().connection, Connection::Failed(_)));
    let _hub = Client::connect_or_start(&stack.hub.paths, &daemon, "test").unwrap();

    ui.click("try-hub-again");

    ui.wait("the connection", |ui| matches!(ui.view().connection, Connection::Ready(_)));
}

#[gpui_kit::test]
fn a_failure_from_the_hub_is_shown_rather_than_lost(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let repo = stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    // The repository disappears, so a new worktree can't be made from it.
    std::fs::remove_dir_all(stack.repo()).unwrap();
    let id = format!("new-in-{}", repo.id);
    ui.click(SharedString::from(id));
    ui.wait("the error", |ui| ui.view().error.is_some());
    assert!(ui.view().error.clone().unwrap().contains("couldn't create the workspace"));
    assert!(!ui.view().busy, "the button works again");
}

#[gpui_kit::test]
fn an_approval_waiting_when_the_app_opens_can_be_answered(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let workspace = client.workspace_create(&repo.id, None, None, None).unwrap();
    let session = client
        .session_create(&workspace.id, protocol::model::AgentKind::Claude, None, Some("manual".into()), None)
        .unwrap();
    client.session_prompt(&session.id, "tool:echo answered > answered.txt", None).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while client.session_get(&session.id).unwrap().status != SessionStatus::WaitingApproval {
        assert!(Instant::now() < deadline, "the agent never asked");
        std::thread::sleep(Duration::from_millis(20));
    }

    // The app opens after the question was asked, so it has to learn about it from history.
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let seq = ui.next_open_approval();
    ui.click(ElementId::NamedInteger("allow".into(), seq));
    ui.wait("the tool to run", |ui| {
        ui.session_status() == Some(SessionStatus::Idle)
            && PathBuf::from(workspace.path.clone().unwrap()).join("answered.txt").exists()
    });
}

#[gpui_kit::test]
fn losing_the_hub_with_nothing_open_still_reconnects(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    assert!(ui.pane().is_none());
    let pid = stack.crash_hub();
    ui.wait_for_new_hub(&stack, pid);
    // And it's really connected: something added elsewhere still shows up.
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
}
