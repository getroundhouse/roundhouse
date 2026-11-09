//! A hub of another build: one across a real break, an older one the app replaces or finishes an update on, and one
//! someone runs themselves. Each is a stand-in, as `examples/stand_in_hub` describes.

#![cfg(test)]

use desktop::app::{AppView, BuildLine, Connection, UpdateQuestion};
use gpui_kit::TestAppContext;

use crate::support::{Stack, StandInHub, Ui};

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

    let mut ui = Ui::open(cx, &stack);
    // Nor when the app looks again, with the window in the back.
    ui.send_to_back();
    ui.wait_out_a_look();

    assert!(older.0.try_wait().unwrap().is_none());
    assert!(matches!(ui.build_line(), BuildLine::Finishing { .. }));
}

/// An agent finishing is usually what lets an update finish, and the app hears of that at once.
#[gpui_kit::test]
fn an_update_finishes_by_itself_once_the_agents_on_the_older_hub_are_done(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--running", "1"]);
    let mut ui = Ui::open(cx, &stack);
    ui.send_to_back();

    older.set_running(0);

    wait_for_the_apps_own_hub(&mut ui, &mut older);
    assert!(stack.client().hub_status().unwrap().on_demand, "the app started this one because it needed it");
}

/// The records don't show another client leaving, so the app looks every so often.
#[gpui_kit::test]
fn an_update_finishes_by_itself_once_nothing_else_is_using_the_older_hub(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--clients", "2"]);
    let mut ui = Ui::open(cx, &stack);
    ui.send_to_back();
    assert!(matches!(ui.build_line(), BuildLine::Finishing { agents: 0, .. }));

    older.set_clients(1);
    ui.wait_out_a_look();

    wait_for_the_apps_own_hub(&mut ui, &mut older);
}

/// Finishing swaps what's on screen for a moment, so it waits for someone to go to another app.
#[gpui_kit::test]
fn an_update_waits_to_finish_by_itself_until_the_window_is_not_in_front(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--clients", "2"]);
    let mut ui = Ui::open(cx, &stack);
    ui.bring_to_front();
    older.set_clients(1);

    ui.wait_out_a_look();

    assert!(matches!(ui.build_line(), BuildLine::Finishing { .. }));
    ui.send_to_back();
    wait_for_the_apps_own_hub(&mut ui, &mut older);
}

/// Finishing an idle hub stops nothing, so a question left open in the card doesn't hold it up.
#[gpui_kit::test]
fn an_update_finishes_by_itself_while_the_card_asks_whether_to_finish_it_now(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--clients", "2"]);
    let mut ui = Ui::open(cx, &stack);
    ui.send_to_back();
    ask_to_finish(&mut ui);
    older.set_clients(1);

    ui.wait_out_a_look();

    wait_for_the_apps_own_hub(&mut ui, &mut older);
    assert_eq!(ui.view().update_question, None, "the question goes with the card");
}

fn wait_for_the_apps_own_hub(ui: &mut Ui<'_>, older: &mut StandInHub) {
    ui.wait("the app's own hub", |ui| {
        older.0.try_wait().unwrap().is_some() && matches!(ui.build_line(), BuildLine::Current { .. })
    });
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

    let detail = ask_to_finish(&mut ui);
    assert!(detail.starts_with("This stops the 3 agents still running on the previous version."), "{detail}");
    assert!(ui.has("update-question"));
    assert!(!ui.cx.has_pending_prompt(), "the card asks, never a dialog");
    ui.click("update-go-ahead");

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
    assert!(!ui.has("finish-update"), "the question takes its place");
    ui.click("update-not-now");

    assert!(!ui.has("update-question"));
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
    assert!(!ui.has("update-go-ahead"), "the question went with the card");
    ui.cx.update(|cx| ui.view.update(cx, AppView::finish_update_now));
    ui.frame();

    assert!(matches!(ui.view().connection, Connection::Ready(_)));
    assert_eq!(stack.client().hub_status().unwrap().pid, pid);
}

/// The card doesn't hold anything else up, so more can start while it asks, and it says so before stopping it.
#[gpui_kit::test]
fn finishing_now_asks_again_when_more_has_started_since_it_asked(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--running", "1"]);
    let mut ui = Ui::open(cx, &stack);
    assert!(ask_to_finish(&mut ui).starts_with("This stops the 1 agent "));
    older.set_running(2);

    ui.click("update-go-ahead");

    ui.wait("the question again", |ui| question_detail(ui).is_some_and(|d| d.starts_with("This stops the 2 agents ")));
    assert!(older.0.try_wait().unwrap().is_none());
    ui.click("update-go-ahead");
    ui.wait("a new hub", |ui| {
        older.0.try_wait().unwrap().is_some() && matches!(ui.view().connection, Connection::Ready(_))
    });
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

/// Presses Finish now in the sidebar, and returns what the card says finishing stops.
fn ask_to_finish(ui: &mut Ui<'_>) -> String {
    ui.click("finish-update");
    ui.wait("the question", |ui| question_detail(ui).is_some());
    question_detail(ui).unwrap()
}

/// What the card says finishing the update stops, while it asks.
fn question_detail(ui: &Ui<'_>) -> Option<String> {
    let question = ui.view().update_question.clone()?;
    assert!(matches!(question, UpdateQuestion::Finish(_)));
    assert_eq!(question.title(), "Finish the update now?");
    Some(question.detail())
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
