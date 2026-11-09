//! A hub of another build: one across a real break, an older one the app replaces or finishes an update on, and one
//! someone runs themselves. Each is a stand-in, as `examples/stand_in_hub` describes.

#![cfg(test)]

use desktop::app::{BuildLine, Connection};
use gpui_kit::TestAppContext;

use crate::support::{Stack, Ui};

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

#[gpui_kit::test]
fn an_older_hub_with_an_agent_running_is_kept(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--running", "1"]);

    let _ui = Ui::open(cx, &stack);

    assert!(older.0.try_wait().unwrap().is_none());
}

/// Rather than wait for it to be idle, which a busy hub may not be for a long time. The app says what's running then,
/// rather than when it connected.
#[gpui_kit::test]
fn an_update_can_be_finished_from_the_sidebar_while_agents_run_on_the_older_hub(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--running", "1"]);
    let mut ui = Ui::open(cx, &stack);
    older.set_running(3);

    let (message, detail) = ask_to_finish(&mut ui);
    assert_eq!(message, "Finish the update now?");
    assert!(detail.starts_with("This stops the 3 agents still running on the previous version."), "{detail}");
    ui.cx.simulate_prompt_answer("Finish now");

    ui.wait("a new hub", |ui| {
        older.0.try_wait().unwrap().is_some() && matches!(ui.view().connection, Connection::Ready(_))
    });
    assert!(stack.client().hub_status().unwrap().on_demand, "the app started this one because it needed it");
    assert!(!ui.has("finish-update"), "the new hub is this app's own build");
}

#[gpui_kit::test]
fn not_now_leaves_the_older_hub_running(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--running", "1"]);
    let mut ui = Ui::open(cx, &stack);

    ask_to_finish(&mut ui);
    // Pressing again while it asks doesn't ask twice.
    ui.click("finish-update");
    ui.cx.simulate_prompt_answer("Not now");
    ui.frame();

    assert!(!ui.cx.has_pending_prompt());
    assert!(matches!(ui.view().connection, Connection::Ready(_)));
    assert!(older.0.try_wait().unwrap().is_none());
    assert!(ui.has("finish-update"), "it can still be finished later");
}

/// The hub it asked about went, and the app started its own, which finishing mustn't stop.
#[gpui_kit::test]
fn finishing_after_the_older_hub_has_gone_leaves_the_new_one_alone(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let _older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--running", "1"]);
    let mut ui = Ui::open(cx, &stack);
    ask_to_finish(&mut ui);

    client_core::stop_hub(&stack.hub.paths.socket).unwrap();
    ui.wait("the app's own hub", |ui| {
        matches!(ui.view().connection, Connection::Ready(_)) && matches!(ui.build_line(), BuildLine::Current { .. })
    });
    let pid = stack.client().hub_status().unwrap().pid;
    ui.cx.simulate_prompt_answer("Finish now");
    ui.frame();

    assert!(matches!(ui.view().connection, Connection::Ready(_)));
    assert_eq!(stack.client().hub_status().unwrap().pid, pid);
}

/// The rules for which hub can be restarted are tested in `hub_source`. This is that the sidebar follows them.
#[gpui_kit::test]
fn an_older_hub_started_by_hand_is_not_offered_to_finish(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let _by_hand = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--not-on-demand"]);
    let mut ui = Ui::open(cx, &stack);

    assert!(matches!(ui.build_line(), BuildLine::OtherHub { .. }));
    assert!(!ui.has("finish-update"));
}

/// Presses Finish now in the sidebar, and returns what the app asks.
fn ask_to_finish(ui: &mut Ui<'_>) -> (String, String) {
    ui.click("finish-update");
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    ui.cx.pending_prompt().unwrap()
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
    client_core::stop_hub(&stack.hub.paths.socket).unwrap();
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
    client_core::stop_hub(&stack.hub.paths.socket).unwrap();

    ui.click("restart-hub");

    ui.wait("a new hub", |ui| matches!(ui.view().connection, Connection::Ready(_)));
}
