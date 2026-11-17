//! A hub of another build: one across a real break, an older one the app replaces or finishes an update on by itself,
//! and one someone runs themselves. Finishing an update by hand is in `restart_now`. Each is a stand-in, as
//! `tools/stand-ins/stand-in-hub/` describes.

#![cfg(test)]

use desktop::app::{Agents, BuildLine, Connection, Updating};
use gpui_kit::TestAppContext;

use crate::support::{Stack, StandInHub, Ui};

#[gpui_kit::test]
fn a_hub_across_a_real_break_is_explained_and_can_be_restarted(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let other = stack.start_stand_in_hub(&["--running", "2"]);
    let mut ui = Ui::open_without_waiting(cx, &stack);

    ui.wait(
        "the app to see the hub",
        |ui| matches!(ui.view().connection(), Connection::AcrossABreak(status) if status.running_sessions == 2),
    );
    ui.click("restart-hub");

    ui.wait("a new hub", |ui| matches!(ui.view().connection(), Connection::Ready(_)));
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
    assert!(matches!(ui.build_line().update, Some(Updating::Finishing { .. })));
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
    assert!(matches!(ui.build_line().update, Some(Updating::Finishing { .. })));

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
    ui.wait("the app to connect", |ui| matches!(ui.view().connection(), Connection::Ready(_)));
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

/// The records don't show a request ending, such as one creating a workspace, so the app looks every so often.
#[gpui_kit::test]
fn an_update_finishes_by_itself_once_nothing_else_is_under_way_on_the_older_hub(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--requests", "1"]);
    let mut ui = Ui::open(cx, &stack);
    ui.bring_to_front();
    assert!(matches!(
        ui.build_line().update,
        Some(Updating::Finishing { agents: Agents { working: 0, waiting_on_you: 0 }, .. })
    ));

    older.set_requests(0);
    ui.wait_out_a_look();

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
        refused && matches!(ui.view().connection(), Connection::Ready(_))
    });
    assert!(older.0.try_wait().unwrap().is_none());
    assert!(matches!(ui.build_line().update, Some(Updating::Finishing { .. })));
}

/// The app and its hub are the same build, with no update to say anything about.
pub(crate) fn on_its_own_build(line: &BuildLine) -> bool {
    line.other_hub.is_none() && line.update.is_none()
}

/// The older hub going, and then the app on its own, apart, so a test that times out says which never came.
pub(crate) fn wait_for_the_apps_own_hub(ui: &mut Ui<'_>, older: &mut StandInHub) {
    ui.wait("the older hub to go", |_| older.0.try_wait().unwrap().is_some());
    ui.wait("the app's own hub", |ui| on_its_own_build(&ui.build_line()));
}

/// The rules for which hub can be restarted are tested in `hub_source`. This is that the sidebar follows them.
#[gpui_kit::test]
fn an_older_hub_started_by_hand_is_not_offered_to_finish(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let _by_hand = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--not-on-demand"]);
    let mut ui = Ui::open(cx, &stack);

    assert!(ui.build_line().other_hub.is_some());
    assert!(!ui.has("finish-update"));
}
#[gpui_kit::test]
fn a_hub_started_by_hand_is_explained_and_left_alone(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut other = stack.start_stand_in_hub(&["--not-on-demand"]);
    let mut ui = Ui::open_without_waiting(cx, &stack);

    ui.wait("the app to see the hub", |ui| matches!(ui.view().connection(), Connection::AcrossABreak(_)));

    assert!(!ui.has("restart-hub"));
    assert!(other.0.try_wait().unwrap().is_none());
    // Whoever runs it stops it, and the app can then start one of its own.
    client_core::stop_hub(&stack.hub.paths.socket).unwrap();
    ui.click("try-hub-again");
    ui.wait("a new hub", |ui| matches!(ui.view().connection(), Connection::Ready(_)));
}

#[gpui_kit::test]
fn a_hub_newer_than_the_app_is_not_offered_for_restart(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _newer = stack.start_stand_in_hub(&["--newer"]);
    let mut ui = Ui::open_without_waiting(cx, &stack);

    ui.wait("the app to see the hub", |ui| matches!(ui.view().connection(), Connection::AcrossABreak(_)));

    assert!(!ui.has("restart-hub"));
    assert!(ui.has("try-hub-again"));
}

#[gpui_kit::test]
fn restarting_a_hub_that_has_already_gone_still_connects(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _other = stack.start_stand_in_hub(&[]);
    let mut ui = Ui::open_without_waiting(cx, &stack);
    ui.wait("the app to see the hub", |ui| matches!(ui.view().connection(), Connection::AcrossABreak(_)));
    client_core::stop_hub(&stack.hub.paths.socket).unwrap();

    ui.click("restart-hub");

    ui.wait("a new hub", |ui| matches!(ui.view().connection(), Connection::Ready(_)));
}
