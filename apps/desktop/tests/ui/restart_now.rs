//! Finishing an update by hand, with `Restart now` on an older hub's card, and the dialog that asks first when that
//! would stop or cut something off. Each hub is a stand-in, as `tools/stand-ins/stand-in-hub/` describes.

#![cfg(test)]

use desktop::app::{Connection, UpdateQuestion};
use gpui_kit::TestAppContext;

use crate::other_hubs::{on_its_own_build, wait_for_the_apps_own_hub};
use crate::support::{Stack, Ui};

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

/// Nothing is using it, so there's nothing to ask, and asking would read as if something would stop. A request under
/// way is what kept this one, which the app only hears has ended when it next looks.
#[gpui_kit::test]
fn restart_now_finishes_at_once_when_nothing_is_using_the_older_hub(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--requests", "1"]);
    let mut ui = Ui::open(cx, &stack);
    older.set_requests(0);

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
    let mut older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--requests", "1"]);
    let mut ui = Ui::open(cx, &stack);

    let detail = ask_to_finish(&mut ui);

    assert!(detail.ends_with("such as creating or archiving a workspace. Restarting now cuts that off."), "{detail}");
    assert!(older.0.try_wait().unwrap().is_none());
    ui.click("restart-now-confirm");
    wait_for_the_apps_own_hub(&mut ui, &mut older);
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
        older.0.try_wait().unwrap().is_some() && matches!(ui.view().connection(), Connection::Ready(_))
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
    assert!(matches!(ui.view().connection(), Connection::Ready(_)));
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
        matches!(ui.view().connection(), Connection::Ready(_)) && on_its_own_build(&ui.build_line())
    });
    let pid = stack.client().hub_status().unwrap().pid;
    assert!(!ui.has_dialog(), "the dialog went with the hub it asked about");
    ui.cx.update(|cx| ui.view.update(cx, |view, cx| view.finish_update_now(&said, cx)));
    ui.frame();

    assert!(matches!(ui.view().connection(), Connection::Ready(_)));
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
        older.0.try_wait().unwrap().is_some() && matches!(ui.view().connection(), Connection::Ready(_))
    });
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
