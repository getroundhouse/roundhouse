//! Losing the hub, and what's waiting when the app comes back.

#![cfg(test)]

use std::path::PathBuf;

use desktop::app::Connection;
use gpui_kit::{AppContext as _, SharedString, TestAppContext};
use protocol::model::SessionStatus;

use crate::support::{Diff, Stack, TIMEOUT, Ui};

/// The same workspace view carries on on the new hub, with the history it had and what was typed: a comment on the
/// diff, which is only in the app until it's sent, and each chat's composer, which is in the app as well as the hub,
/// which keeps it once it's had the change.
#[gpui_kit::test]
fn losing_the_hub_starts_a_new_one_and_picks_up_where_it_left_off_with_whats_typed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let id = ui.new_workspace();
    let first = ui.active_chat();
    ui.input("say:before");
    ui.press("enter");
    ui.wait_idle_with("the first turn", |text| text.contains("before"));
    let second = ui.new_chat();
    ui.input("say:from the second chat");
    ui.show_chat(&first);
    ui.frame();
    ui.input("say:after");
    std::fs::write(ui.worktree(&id).join("a.txt"), "one\ntwo\n").unwrap();
    let diff = ui.open_changes();
    ui.open_diff_of(&diff, "a.txt");
    ui.start_comment(&diff, "a.txt", 2);
    ui.input("Why");
    let (pane, transcript) = (ui.pane().unwrap(), ui.transcript().unwrap());

    let pid = stack.crash_hub();

    ui.wait_for_new_hub(&stack, pid);
    assert_eq!(ui.pane().unwrap().entity_id(), pane.entity_id(), "the same workspace view");
    assert_eq!(ui.transcript().unwrap().entity_id(), transcript.entity_id(), "the same chat's view");
    // The comment still has the keys, so the rest of it doesn't go to the agent, and saves in the new hub.
    ui.input(" two?");
    ui.press("enter");
    ui.wait("the comment to be saved", |_| {
        stack.client().comment_list(&id).unwrap().iter().map(|c| c.text.clone()).collect::<Vec<_>>() == ["Why two?"]
    });
    assert_eq!(ui.composer_text(), "say:after", "the first chat's draft");
    ui.wait("the history", |ui| ui.assistant_text().contains("before"));
    // Each chat keeps its draft, and sends it on the new hub.
    ui.show_chat(&second);
    ui.frame();
    assert_eq!(ui.composer_text(), "say:from the second chat", "the second chat's draft");
    ui.press("enter");
    ui.wait_idle_with("the second chat's reply", |text| text.contains("from the second chat"));
    ui.show_chat(&first);
    ui.frame();
    ui.press("enter");
    ui.wait_idle_with("the first chat's next turn", |text| text.contains("after"));
}

/// After a reconnect, a pane reads the worktree again once it's on screen, so the hub doesn't start a worker for
/// every pane at once.
#[gpui_kit::test]
fn after_a_reconnect_panes_catch_up_when_shown_and_an_archived_ones_goes(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let daemon = stack.without_a_daemon();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let archived = ui.new_workspace();
    let hidden = ui.new_workspace();
    ui.input("say:still here");
    let hidden_diff = open_empty_diff(&mut ui);
    let shown = ui.new_workspace();
    let shown_diff = open_empty_diff(&mut ui);
    ui.lose_the_hub(&stack);
    // A kept pane does nothing until there's a connection for it, even when a shortcut reaches the window.
    ui.cx.update_window(ui.handle, |_, window, cx| window.blur(cx)).unwrap();
    ui.frame();
    ui.press("secondary-shift-b");
    assert!(ui.changes_open());
    let client = local_hub::connect_or_start(&stack.hub.paths, &daemon, "test").unwrap();
    client.workspace_archive(&archived, false).unwrap();
    for id in [&hidden, &shown] {
        std::fs::write(ui.worktree(id).join("a.txt"), "one\nchanged while away\n").unwrap();
    }

    ui.click("try-hub-again");

    ui.wait("the pane on screen to catch up", |ui| {
        matches!(ui.view().connection(), Connection::Ready(_)) && shows_changes(ui, &shown_diff)
    });
    assert!(!ui.view().panes.contains_key(&archived));
    assert!(!shows_changes(&ui, &hidden_diff), "it waits until it's shown");
    ui.click(SharedString::from(format!("workspace-{hidden}")));
    ui.wait("the other pane to catch up once shown", |ui| shows_changes(ui, &hidden_diff));
    assert_eq!(ui.composer_text(), "say:still here");
}

/// A pane still behind from one reconnect, and shown while the hub's gone again, doesn't try the lost
/// connection. It catches up on the next. While the hub is gone, the heading stays over the repositories, which are
/// still listed, and Add project waits for it.
#[gpui_kit::test]
fn a_pane_shown_while_the_hub_is_gone_catches_up_once_it_is_back(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let daemon = stack.without_a_daemon();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let behind = ui.new_workspace();
    let diff = open_empty_diff(&mut ui);
    ui.new_workspace();
    assert!(ui.on("add-project"), "with a hub, a project can be added");
    ui.lose_the_hub(&stack);
    assert_eq!(ui.view().records().repos().len(), 1, "the repository is still listed");
    assert!(ui.has("add-project"), "so the heading stays over it");
    assert!(ui.off("add-project"), "with no hub to add one to");
    local_hub::connect_or_start(&stack.hub.paths, &daemon, "test").unwrap();
    ui.click("try-hub-again");
    ui.wait("the hub to come back", |ui| matches!(ui.view().connection(), Connection::Ready(_)));
    ui.lose_the_hub(&stack);
    std::fs::write(ui.worktree(&behind).join("a.txt"), "one\nchanged while away\n").unwrap();

    ui.click(SharedString::from(format!("workspace-{behind}")));
    ui.frame();

    assert_eq!(ui.view().selected(), Some(&behind));
    assert_eq!(ui.view().error.as_deref(), None);
    local_hub::connect_or_start(&stack.hub.paths, &daemon, "test").unwrap();
    ui.click("try-hub-again");
    ui.wait("it to catch up", |ui| matches!(ui.view().connection(), Connection::Ready(_)) && shows_changes(ui, &diff));
    assert_eq!(ui.view().error.as_deref(), None);
}

/// Opens the selected workspace's diff, which has nothing in it yet.
fn open_empty_diff(ui: &mut Ui<'_>) -> Diff {
    let diff = ui.open_changes();
    assert!(!shows_changes(ui, &diff));
    diff
}

fn shows_changes(ui: &Ui<'_>, diff: &Diff) -> bool {
    ui.cx.read_entity(&diff.review, |r, _| r.listing().is_some_and(|listing| listing.changed() > 0))
}

#[gpui_kit::test]
fn a_failed_connection_can_be_tried_again(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    let daemon = stack.without_a_daemon();
    let mut ui = Ui::open_without_waiting(cx, &stack);
    ui.wait("the failure", |ui| matches!(ui.view().connection(), Connection::Failed(_)));
    let _hub = local_hub::connect_or_start(&stack.hub.paths, &daemon, "test").unwrap();

    ui.click("try-hub-again");

    ui.wait("the connection", |ui| matches!(ui.view().connection(), Connection::Ready(_)));
}

#[gpui_kit::test]
fn an_approval_waiting_when_the_app_opens_can_be_answered(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let workspace = client.workspace_create(&repo.id, None, None, None).unwrap();
    let session = client
        .session_create(
            &workspace.id,
            protocol::model::AgentKind::Claude,
            protocol::capability::settings::Settings {
                mode: Some(protocol::fixtures::plan_mode()),
                ..protocol::capability::settings::Settings::default()
            },
            None,
        )
        .unwrap();
    client.session_prompt(&session.id, "tool:echo answered > answered.txt", None).unwrap();
    waiting::thread::until(TIMEOUT, "the agent never asked", || {
        client.session_get(&session.id).unwrap().status == SessionStatus::WaitingApproval
    });

    // The app opens after the question was asked, so it has to learn about it from history.
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    ui.allow_tool_use();
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
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
}

/// The app restarts an older hub the moment it's idle, which is often just as someone starts typing their next
/// prompt. The workspace stays on screen meanwhile, so nothing typed is lost. Enter waits too: the hub may be one
/// that's being stopped, which would take the prompt and then cut its agent off.
#[gpui_kit::test]
fn whats_typed_while_the_hub_restarts_is_kept_and_waits_to_be_sent(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    ui.input("say:be");
    let pid = stack.client().hub_status().unwrap().pid;

    // Typed and sent before anything else runs, so the app is surely still connecting.
    ui.cx.update(|cx| ui.view.update(cx, |view, cx| view.restart_hub(pid, cx)));
    assert!(matches!(ui.view().connection(), Connection::Connecting));
    ui.input("fore");
    ui.press("enter");

    assert_eq!(ui.composer_text(), "say:before", "typed, and not sent");
    ui.wait_for_new_hub(&stack, pid);
    assert_eq!(ui.composer_text(), "say:before");
    assert!(ui.events().is_empty(), "nothing reached either hub");
    // The composer still has the keys, and sends once the hub is back.
    ui.press("enter");
    ui.wait_idle_with("the reply", |text| text.contains("before"));
}

/// The app a test drives is still running as the test ends, and connects again as the test stops its hub, which would
/// start a hub that nothing stops, in a directory the test removes. The stack's daemon refuses it.
#[gpui_kit::test]
fn a_test_that_ends_with_its_app_open_leaves_no_hub_running(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    let (dir, socket) = (stack.dir.path().to_path_buf(), stack.hub.paths.socket.clone());

    drop(stack);

    ui.wait("the app to be refused a hub", |ui| matches!(ui.view().connection(), Connection::Failed(_)));
    assert!(client_core::Client::connect(&socket, "test").is_err(), "no hub is running there");
    assert!(!dir.exists(), "nothing made its directory again");
}
