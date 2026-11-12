//! What the app shows about updating itself. Only an app Velopack installed updates for real, so
//! a failure here is made up, and a check finds a build that doesn't update.

#![cfg(test)]

use desktop::app::UpdateQuestion;
use desktop::updates::{Kind, Problem};
use gpui_kit::TestAppContext;

use crate::menus::{CHECK_FOR_UPDATES, choose};
use crate::support::{Frozen, Stack, Ui};

#[gpui_kit::test]
fn a_failed_update_shows_in_the_sidebar(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    assert!(!ui.has("update-problem"));

    let problem = Problem { kind: Kind::Unreachable, detail: "connection refused".to_string() };
    ui.cx.update(|cx| desktop::updates::show_problem(problem, cx));

    assert!(ui.has("update-problem"));
}

#[gpui_kit::test]
fn a_check_under_way_shows_in_the_sidebar(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    assert!(!ui.has("checking-for-updates"));

    ui.cx.update(desktop::updates::show_checking);

    assert!(ui.has("checking-for-updates"));
}

#[gpui_kit::test]
fn a_downloaded_update_shows_as_a_card_in_the_versions_place(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    assert!(ui.has("build-line"));
    assert!(!ui.has("update-card"));

    ui.cx.update(|cx| desktop::updates::show_ready("9.9.9", cx));

    assert!(ui.has("update-card"));
    assert!(!ui.has("build-line"));
}

/// Opens a workspace with something typed in its chat, and an update downloaded.
fn typing_with_an_update_ready<'a>(cx: &'a mut TestAppContext, stack: &Stack, typed: &str) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    ui.input(typed);
    ui.cx.update(|cx| desktop::updates::show_ready("9.9.9", cx));
    ui
}

/// Starts a comment on the diff, then stops the hub, so what's typed in it can't reach the hub until this goes, and
/// restarting would lose it.
fn start_a_comment(ui: &mut Ui<'_>, stack: &Stack) -> Frozen {
    let workspace = ui.view().selected.clone().unwrap();
    std::fs::write(ui.worktree(&workspace).join("a.txt"), "one\ntwo\n").unwrap();
    let diff = ui.open_changes();
    ui.open_diff_of(&diff, "a.txt");
    ui.start_comment(&diff, "a.txt", 2);
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    let frozen = Frozen::stop(stack.client().hub_status().unwrap().pid);
    ui.input("Why this word?");
    frozen
}

/// What the card asks before restarting to update, while it asks.
fn restart_question(ui: &Ui<'_>) -> Option<(&'static str, String)> {
    let question = ui.view().update_question.clone()?;
    assert!(matches!(question, UpdateQuestion::Restart { .. }));
    Some((question.title(), question.detail()))
}

/// In the card, in place of its button, rather than in a dialog.
#[gpui_kit::test]
fn restarting_to_update_asks_first_when_something_would_be_lost(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing_with_an_update_ready(cx, &stack, "say:not sent yet");
    let _frozen = start_a_comment(&mut ui, &stack);

    ui.click("restart-to-update");

    let (title, detail) = restart_question(&ui).unwrap();
    assert_eq!(title, "Restart to update?");
    assert_eq!(detail, "Something in 1 comment hasn't been sent, and restarting loses it.");
    assert!(ui.has("update-question"));
    assert!(!ui.has("restart-to-update"));
    assert!(!ui.cx.has_pending_prompt());
    ui.click("update-not-now");
    assert_eq!(restart_question(&ui), None);
    assert!(ui.has("restart-to-update"));
    assert!(!ui.has("update-problem"), "it didn't try to install");
    assert_eq!(ui.composer_text(), "say:not sent yet");
}

/// The hub keeps what's typed and attached in a composer, and restarting waits for it to get there, so restarting
/// loses none of it.
#[gpui_kit::test]
fn restarting_to_update_doesnt_ask_about_whats_typed_or_attached_in_a_composer(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing_with_an_update_ready(cx, &stack, "say:kept");
    ui.attach_a_file("build.log", "error at line 42\n");

    ui.click("restart-to-update");

    assert_eq!(restart_question(&ui), None);
    assert!(ui.has("update-problem"));
}

/// What's typed and not yet kept, since typing hasn't paused, is on its way to the hub, so restarting waits for it
/// rather than asking.
#[gpui_kit::test]
fn restarting_to_update_waits_for_whats_on_its_way_to_the_hub(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing_with_an_update_ready(cx, &stack, "say:kept");
    let chat = ui.active_chat();
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    // Once the hub has answered a change, the one after it waits for typing to pause. One letter, as typing several
    // can take longer than the pause in a test.
    ui.input("!");

    ui.click("restart-to-update");

    ui.wait("it to go ahead", |ui| ui.has("update-problem"));
    assert_eq!(restart_question(&ui), None);
    assert_eq!(stack.client().session_get(&chat).unwrap().draft.text, "say:kept!");
}

/// The hub keeps a comment being written too, so restarting doesn't ask about one it has.
#[gpui_kit::test]
fn restarting_to_update_doesnt_ask_about_a_comment_the_hub_has(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing_with_an_update_ready(cx, &stack, "");
    let workspace = ui.view().selected.clone().unwrap();
    std::fs::write(ui.worktree(&workspace).join("a.txt"), "one\ntwo\n").unwrap();
    let diff = ui.open_changes();
    ui.open_diff_of(&diff, "a.txt");
    ui.start_comment(&diff, "a.txt", 2);
    ui.input("Why this word?");

    ui.click("restart-to-update");

    ui.wait("it to go ahead", |ui| ui.has("update-problem"));
    assert_eq!(restart_question(&ui), None);
}

/// Opens the new-workspace dialog with an update downloaded.
fn the_dialog_with_an_update_ready<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = Ui::open_with_a_repository(cx, stack);
    ui.cx.update(|cx| desktop::updates::show_ready("9.9.9", cx));
    ui.open_new_workspace();
    ui
}

/// What's left in the new-workspace dialog is for a first chat, and restarting would lose what the hub hasn't had of
/// it, as when the hub doesn't answer.
#[gpui_kit::test]
fn restarting_to_update_counts_what_the_hub_hasnt_had_of_the_new_workspace_dialog(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = the_dialog_with_an_update_ready(cx, &stack);
    let _frozen = Frozen::stop(stack.client().hub_status().unwrap().pid);
    ui.input("half a thought");
    ui.press("escape");

    ui.click("restart-to-update");

    let (_, detail) = restart_question(&ui).unwrap();
    assert_eq!(detail, "Something in 1 chat hasn't been sent, and restarting loses it.");
}

#[gpui_kit::test]
fn restarting_to_update_doesnt_ask_about_whats_in_the_new_workspace_dialog_once_the_hub_has_it(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let mut ui = the_dialog_with_an_update_ready(cx, &stack);
    ui.input("half a thought");
    ui.press("escape");

    ui.click("restart-to-update");

    ui.wait("it to go ahead", |ui| ui.has("update-problem"));
    assert_eq!(restart_question(&ui), None);
}

/// The card doesn't hold anything else up, so more can be typed while it asks, and it says so before losing it.
#[gpui_kit::test]
fn restarting_anyway_asks_again_when_more_would_be_lost_since_it_asked(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing_with_an_update_ready(cx, &stack, "say:not sent yet");
    let _frozen = start_a_comment(&mut ui, &stack);
    ui.click("restart-to-update");
    ui.open_new_workspace();
    ui.input("half a thought");
    ui.press("escape");

    ui.click("update-go-ahead");

    let (_, detail) = restart_question(&ui).unwrap();
    assert_eq!(detail, "Something in 1 chat and 1 comment hasn't been sent, and restarting loses it.");
    assert!(!ui.has("update-problem"), "it didn't try to install");
    // Only an app Velopack installed can install an update, so here it says it couldn't, which shows it went ahead.
    ui.click("update-go-ahead");
    assert_eq!(restart_question(&ui), None);
    assert!(ui.has("update-problem"));
}

/// Only an app Velopack installed can install an update, so here it says it couldn't, which shows it went ahead.
#[gpui_kit::test]
fn restarting_to_update_with_nothing_typed_asks_nothing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing_with_an_update_ready(cx, &stack, "  ");
    assert!(!ui.has("update-problem"));

    ui.click("restart-to-update");

    assert_eq!(restart_question(&ui), None);
    assert!(ui.has("update-problem"));
}

/// Under the build, for a few seconds, rather than in an alert.
#[gpui_kit::test]
fn checking_for_updates_in_a_build_that_doesnt_update_says_so_in_the_sidebar(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    assert!(!ui.has("update-answer"));

    choose(&mut ui, CHECK_FOR_UPDATES.0, CHECK_FOR_UPDATES.1);

    assert!(ui.has("update-answer"));
    assert!(!ui.cx.has_pending_prompt());
}

/// The sidebar already says why, which is the answer.
#[gpui_kit::test]
fn checking_for_updates_where_the_app_cant_update_adds_nothing_to_why(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    let problem = Problem { kind: Kind::Translocated, detail: "/private/var/folders/AppTranslocation".to_string() };
    ui.cx.update(|cx| desktop::updates::show_problem(problem, cx));

    choose(&mut ui, CHECK_FOR_UPDATES.0, CHECK_FOR_UPDATES.1);

    assert!(ui.has("update-problem"));
    assert!(!ui.has("update-answer"));
    assert!(!ui.cx.has_pending_prompt());
}
