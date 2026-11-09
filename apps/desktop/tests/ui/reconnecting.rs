//! Losing the hub, and what's waiting when the app comes back.

#![cfg(test)]

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use client_core::Client;
use desktop::app::Connection;
use gpui_kit::component::diff::{DiffLinePosition, DiffSide};
use gpui_kit::{AppContext as _, ElementId, SharedString, TestAppContext};
use protocol::model::SessionStatus;

use crate::support::{Stack, TIMEOUT, Ui};

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

    let pid = stack.client().hub_status().unwrap().pid;
    let killed = Command::new("kill").args(["-9", &pid.to_string()]).status().unwrap();
    assert!(killed.success());

    ui.wait("a new hub", |ui| {
        matches!(ui.view().connection, Connection::Ready(_))
            && stack.client().hub_status().is_ok_and(|status| status.pid != pid)
            && ui.transcript().is_some()
    });
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
    start_comment(&mut ui, "a.txt", 2);
    ui.input("Why");

    let pid = stack.client().hub_status().unwrap().pid;
    client_core::stop_hub(&stack.hub.paths.socket).unwrap();
    ui.wait("a new hub", |ui| {
        matches!(ui.view().connection, Connection::Ready(_))
            && Client::connect(&stack.hub.paths.socket, "test")
                .is_ok_and(|c| c.hub_status().is_ok_and(|s| s.pid != pid))
            && ui.transcript().is_some()
    });

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

/// Opens the diff, and starts a comment on a line on its new side, as that line's add button would.
fn start_comment(ui: &mut Ui<'_>, path: &str, line: usize) {
    ui.click("toggle-diff");
    let pane = ui.pane().unwrap();
    ui.wait("the diff panel", |ui| ui.cx.read_entity(&pane, |p, _| p.diff.is_some()));
    let diff = ui.cx.read_entity(&pane, |p, _| p.diff.clone()).unwrap();
    ui.wait("the diff to load", |ui| ui.cx.read_entity(&diff, |d, _| d.state.is_some()));
    let position = DiffLinePosition::new(path, DiffSide::Modified, line);
    ui.cx
        .update_window(ui.handle, |_, window, cx| diff.update(cx, |d, cx| d.start_comment(position, window, cx)))
        .unwrap();
    ui.frame();
}

#[gpui_kit::test]
fn a_workspace_archived_while_the_app_was_away_takes_its_pane_and_the_rest_catch_up(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    // So the app can't start a hub of its own, and waits while this archives a workspace.
    let daemon = std::mem::replace(&mut stack.hub.daemon, "/nonexistent/roundhoused".into());
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let kept = ui.new_workspace();
    ui.input("say:still here");
    ui.click("toggle-diff");
    let pane = ui.pane().unwrap();
    let has_changes = |ui: &Ui| ui.cx.read_entity(&pane, |p, cx| p.diff.as_ref().map(|d| !d.read(cx).empty));
    ui.wait("the diff, with nothing in it yet", |ui| has_changes(ui) == Some(false));
    let archived = ui.new_workspace();
    client_core::HubAdmin::connect(&stack.hub.paths.socket).unwrap().stop().unwrap();
    ui.wait("the app to find no hub", |ui| matches!(ui.view().connection, Connection::Failed(_)));
    // A kept pane does nothing until there's a connection for it, even when a shortcut reaches the window.
    ui.cx.update_window(ui.handle, |_, window, cx| window.blur(cx)).unwrap();
    ui.frame();
    ui.press("cmd-shift-d");
    assert!(!ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.diff_open));
    let client = Client::connect_or_start(&stack.hub.paths, &daemon, "test").unwrap();
    client.workspace_archive(&archived, false).unwrap();
    std::fs::write(ui.worktree(&kept).join("a.txt"), "one\nchanged while away\n").unwrap();

    ui.click("try-hub-again");

    ui.wait("the connection", |ui| matches!(ui.view().connection, Connection::Ready(_)) && ui.pane().is_some());
    assert!(!ui.view().panes.contains_key(&archived));
    assert_eq!(ui.view().selected, Some(kept));
    assert_eq!(ui.composer_text(), "say:still here");
    // It wasn't on screen when the app reconnected, and catches up now that it is.
    ui.wait("its diff to catch up", |ui| has_changes(ui) == Some(true));
}

#[gpui_kit::test]
fn a_hub_across_a_real_break_is_explained_and_can_be_restarted(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let other = stack.start_stand_in_hub(&["--running", "2"]);
    let mut ui = Ui::open_without_waiting(cx, &stack);

    ui.wait(
        "the app to see the hub",
        |ui| matches!(&ui.view().connection, Connection::AcrossABreak(status) if status.running_sessions == 2),
    );
    ui.click("restart-hub");

    ui.wait("a new hub", |ui| matches!(ui.view().connection, Connection::Ready(_)));
    let status = stack.client().hub_status().unwrap();
    assert_ne!(status.pid, other.0.id());
    assert!(status.on_demand);
}

#[gpui_kit::test]
fn an_older_hub_with_nothing_running_is_replaced_when_the_app_connects(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1"]);

    let _ui = Ui::open(cx, &stack);

    let status = stack.client().hub_status().unwrap();
    assert_ne!(status.pid, older.0.id());
    assert!(status.on_demand, "the app started this one because it needed it");
}

/// The stand-in can't serve the app, which is how this shows the app used it rather than replacing it.
#[gpui_kit::test]
fn an_older_hub_with_an_agent_running_is_kept(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--running", "1"]);
    let mut ui = Ui::open_without_waiting(cx, &stack);

    ui.wait("the app to try that hub", |ui| !matches!(ui.view().connection, Connection::Connecting));

    assert!(older.0.try_wait().unwrap().is_none());
}

#[gpui_kit::test]
fn a_hub_started_by_hand_is_explained_and_left_alone(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut other = stack.start_stand_in_hub(&["--not-on-demand"]);
    let mut ui = Ui::open_without_waiting(cx, &stack);

    ui.wait("the app to see the hub", |ui| matches!(ui.view().connection, Connection::AcrossABreak(_)));

    assert!(!ui.has("restart-hub"));
    assert!(other.0.try_wait().unwrap().is_none());
    // Whoever runs it stops it, and the app can then start one of its own.
    client_core::HubAdmin::connect(&stack.hub.paths.socket).unwrap().stop().unwrap();
    ui.click("try-hub-again");
    ui.wait("a new hub", |ui| matches!(ui.view().connection, Connection::Ready(_)));
}

#[gpui_kit::test]
fn a_hub_newer_than_the_app_is_not_offered_for_restart(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _newer = stack.start_stand_in_hub(&["--newer"]);
    let mut ui = Ui::open_without_waiting(cx, &stack);

    ui.wait("the app to see the hub", |ui| matches!(ui.view().connection, Connection::AcrossABreak(_)));

    assert!(!ui.has("restart-hub"));
    assert!(ui.has("try-hub-again"));
}

#[gpui_kit::test]
fn restarting_a_hub_that_has_already_gone_still_connects(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _other = stack.start_stand_in_hub(&[]);
    let mut ui = Ui::open_without_waiting(cx, &stack);
    ui.wait("the app to see the hub", |ui| matches!(ui.view().connection, Connection::AcrossABreak(_)));
    client_core::HubAdmin::connect(&stack.hub.paths.socket).unwrap().stop().unwrap();

    ui.click("restart-hub");

    ui.wait("a new hub", |ui| matches!(ui.view().connection, Connection::Ready(_)));
}

#[gpui_kit::test]
fn a_failed_connection_can_be_tried_again(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    let daemon = std::mem::replace(&mut stack.hub.daemon, "/nonexistent/roundhoused".into());
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
    let pid = stack.client().hub_status().unwrap().pid;
    assert!(Command::new("kill").args(["-9", &pid.to_string()]).status().unwrap().success());
    ui.wait("a new hub", |ui| {
        matches!(ui.view().connection, Connection::Ready(_))
            && Client::connect(&stack.hub.paths.socket, "test")
                .is_ok_and(|c| c.hub_status().is_ok_and(|s| s.pid != pid))
    });
    // And it's really connected: something added elsewhere still shows up.
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
}
