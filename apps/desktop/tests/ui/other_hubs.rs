//! A hub of another build: one across a real break, an older one the app replaces or finishes an update on, and one
//! someone runs themselves. Each is a stand-in, as `examples/stand_in_hub` describes.

#![cfg(test)]

use desktop::app::{Agents, BuildLine, Connection, UpdateQuestion};
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
    // Nor when the app looks again.
    ui.wait_out_a_look();

    assert!(older.0.try_wait().unwrap().is_none());
    assert!(matches!(ui.build_line(), BuildLine::Finishing { .. }));
}

/// A restart stops none of its agents, so the app doesn't wait for them.
#[gpui_kit::test]
fn an_older_hub_that_keeps_its_workers_is_restarted_at_once_with_agents_running(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let args = ["--speaks-this-protocol", "--build", "1", "--running", "2", "--keeps-workers"];
    let mut older = stack.start_stand_in_hub(&args);

    let mut ui = Ui::open(cx, &stack);

    wait_for_the_apps_own_hub(&mut ui, &mut older);
    assert!(older.restarted(), "asked to restart, rather than to stop");
}

/// It goes once the script is done, as a hub asked to restart does, and until then the app uses it.
#[gpui_kit::test]
fn an_older_hub_that_keeps_its_workers_restarts_once_its_setup_script_is_done(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let args = ["--speaks-this-protocol", "--build", "1", "--running", "2", "--keeps-workers", "--setups", "1"];
    let mut older = stack.start_stand_in_hub(&args);
    let mut ui = Ui::open(cx, &stack);
    ui.wait_out_a_look();
    assert!(older.0.try_wait().unwrap().is_none());
    assert!(matches!(ui.build_line(), BuildLine::Finishing { .. }));

    older.set_setups(0);

    wait_for_the_apps_own_hub(&mut ui, &mut older);
    assert!(older.restarted(), "asked to restart, rather than to stop");
}

/// A script can end between the app seeing it running and asking for the restart, so the hub goes while the app is
/// still joining it, and the app connects to its own.
#[gpui_kit::test]
fn an_older_hub_that_goes_as_the_app_joins_it_leaves_the_app_on_its_own(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let ends = "--setups-end-as-it-restarts";
    let args = ["--speaks-this-protocol", "--build", "1", "--keeps-workers", "--setups", "1", ends];
    let mut older = stack.start_stand_in_hub(&args);

    let mut ui = Ui::open_without_waiting(cx, &stack);

    wait_for_the_apps_own_hub(&mut ui, &mut older);
    ui.wait("the app to connect", |ui| matches!(ui.view().connection, Connection::Ready(_)));
    assert!(older.restarted(), "asked to restart, rather than to stop");
}

/// An agent finishing is usually what lets an update finish, and the app hears of that at once. It takes the chance
/// even with the window in front, since waiting for someone to leave had the update look stuck.
#[gpui_kit::test]
fn an_update_finishes_by_itself_once_the_agents_on_the_older_hub_are_done(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--running", "1"]);
    let mut ui = Ui::open(cx, &stack);
    ui.bring_to_front();

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
    ui.bring_to_front();
    assert!(matches!(ui.build_line(), BuildLine::Finishing { agents: Agents { working: 0, waiting_on_you: 0 }, .. }));

    older.set_clients(1);
    ui.wait_out_a_look();

    wait_for_the_apps_own_hub(&mut ui, &mut older);
}

/// Finishing an idle hub stops nothing, so the dialog left open doesn't hold it up, and goes with the hub it asked
/// about.
#[gpui_kit::test]
fn an_update_finishes_by_itself_while_the_dialog_asks_whether_to_restart_now(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--running", "1"]);
    let mut ui = Ui::open(cx, &stack);
    ask_to_finish(&mut ui);

    older.set_running(0);

    wait_for_the_apps_own_hub(&mut ui, &mut older);
    assert_eq!(ui.view().update_question, None);
    assert!(!ui.has_dialog(), "the dialog goes with the hub it asked about");
}

/// Nothing is using it, so there's nothing to ask, and asking would read as if something would stop. Another client
/// is what kept this one, which the app only hears has gone when it next looks.
#[gpui_kit::test]
fn restart_now_finishes_at_once_when_nothing_is_using_the_older_hub(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--clients", "2"]);
    let mut ui = Ui::open(cx, &stack);
    older.set_clients(1);

    ui.click("finish-update");

    wait_for_the_apps_own_hub(&mut ui, &mut older);
    assert!(!ui.has_dialog());
}

/// The card says "Waiting on other work" then, and restarting cuts that off, so it still asks, and says that rather
/// than that it stops agents.
#[gpui_kit::test]
fn restart_now_says_when_what_it_cuts_off_is_something_else_using_the_older_hub(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--clients", "2"]);
    let mut ui = Ui::open(cx, &stack);

    let detail = ask_to_finish(&mut ui);

    assert!(detail.ends_with("something else is using it, such as the command line. Restarting now cuts that off."));
    assert!(older.0.try_wait().unwrap().is_none());
    ui.click("restart-now-confirm");
    wait_for_the_apps_own_hub(&mut ui, &mut older);
}

/// A hub from the update before this one can't be asked to stop only if it's idle, so the app goes by what the hub
/// said was running, as it did before hubs could decide.
#[gpui_kit::test]
fn an_update_finishes_on_a_hub_from_before_one_could_decide_whether_it_is_idle(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let args = ["--speaks-this-protocol", "--build", "1", "--running", "1", "--asked-to-stop-if-idle", "unknown"];
    let mut older = stack.start_stand_in_hub(&args);
    let mut ui = Ui::open(cx, &stack);

    older.set_running(0);

    wait_for_the_apps_own_hub(&mut ui, &mut older);
}

/// A prompt sent between the app seeing the hub idle and stopping it starts an agent, which finishing the update by
/// itself mustn't stop. The hub refuses, and the app carries on with it, and waits on the agent.
#[gpui_kit::test]
fn an_older_hub_that_is_busy_once_asked_to_stop_is_kept(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let args = ["--speaks-this-protocol", "--build", "1", "--running", "1", "--asked-to-stop-if-idle", "busy"];
    let mut older = stack.start_stand_in_hub(&args);
    let mut ui = Ui::open(cx, &stack);

    older.set_running(0);

    // It says an agent is running again once it has been asked to stop and refused.
    let socket = stack.hub.paths.socket.clone();
    ui.wait("the app to be back on the older hub after it refused", |ui| {
        let refused =
            client_core::HubAdmin::connect(&socket).is_ok_and(|hub| hub.status().unwrap().running_sessions == 1);
        refused && matches!(ui.view().connection, Connection::Ready(_))
    });
    assert!(older.0.try_wait().unwrap().is_none());
    assert!(matches!(ui.build_line(), BuildLine::Finishing { .. }));
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
    assert!(ui.has_dialog());
    assert!(!ui.cx.has_pending_prompt(), "the app's own dialog asks, never the system's");
    ui.click("restart-now-confirm");

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
    ui.click("restart-now-not-now");

    assert!(!ui.has_dialog());
    assert_eq!(ui.view().update_question, None);
    assert!(matches!(ui.view().connection, Connection::Ready(_)));
    assert!(older.0.try_wait().unwrap().is_none());
    assert!(ui.has("finish-update"), "the card is as it was, so it can still be restarted later");

    // Escape is the same as Not now.
    ask_to_finish(&mut ui);
    ui.press("escape");

    assert!(!ui.has_dialog());
    assert_eq!(ui.view().update_question, None);
    assert!(older.0.try_wait().unwrap().is_none());
}

/// The hub it asked about went, and the app started its own, which finishing mustn't stop.
#[gpui_kit::test]
fn finishing_after_the_older_hub_has_gone_leaves_the_new_one_alone(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let _older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--running", "1"]);
    let mut ui = Ui::open(cx, &stack);
    ask_to_finish(&mut ui);
    let Some(UpdateQuestion::Finish(said)) = ui.view().update_question.clone() else { panic!("no question") };

    client_core::stop_hub(&stack.hub.paths.socket).unwrap();
    ui.wait("the app's own hub", |ui| {
        matches!(ui.view().connection, Connection::Ready(_)) && matches!(ui.build_line(), BuildLine::Current { .. })
    });
    let pid = stack.client().hub_status().unwrap().pid;
    assert!(!ui.has_dialog(), "the dialog went with the hub it asked about");
    ui.cx.update(|cx| ui.view.update(cx, |view, cx| view.finish_update_now(&said, cx)));
    ui.frame();

    assert!(matches!(ui.view().connection, Connection::Ready(_)));
    assert_eq!(stack.client().hub_status().unwrap().pid, pid);
}

/// More can start while it asks, and it says so before stopping it.
#[gpui_kit::test]
fn finishing_now_asks_again_when_more_has_started_since_it_asked(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--running", "1"]);
    let mut ui = Ui::open(cx, &stack);
    assert!(ask_to_finish(&mut ui).starts_with("This stops the 1 agent "));
    older.set_running(2);

    ui.click("restart-now-confirm");

    ui.wait("the question again", |ui| question_detail(ui).is_some_and(|d| d.starts_with("This stops the 2 agents ")));
    assert!(older.0.try_wait().unwrap().is_none());
    ui.click("restart-now-confirm");
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

/// Presses Restart now in the sidebar, and returns what the dialog says that stops.
fn ask_to_finish(ui: &mut Ui<'_>) -> String {
    ui.click("finish-update");
    ui.wait("the question", |ui| question_detail(ui).is_some());
    question_detail(ui).unwrap()
}

/// What the dialog says restarting now stops, while it asks.
fn question_detail(ui: &Ui<'_>) -> Option<String> {
    let question = ui.view().update_question.clone()?;
    assert!(matches!(question, UpdateQuestion::Finish(_)));
    assert_eq!(question.title(), "Restart now?");
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
