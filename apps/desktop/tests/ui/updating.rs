//! What the app shows about updating itself. Only an app Velopack installed updates for real, so
//! a failure here is made up, and a check finds a build that doesn't update.

#![cfg(test)]

use desktop::app::UpdateQuestion;
use desktop::updates::{Kind, Problem};
use gpui_kit::TestAppContext;

use crate::menus::choose;
use crate::support::{Stack, Ui};

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
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input(typed);
    ui.cx.update(|cx| desktop::updates::show_ready("9.9.9", cx));
    ui
}

/// What the card asks before restarting to update, while it asks.
fn restart_question(ui: &Ui<'_>) -> Option<(&'static str, String)> {
    let question = ui.view().update_question.clone()?;
    assert!(matches!(question, UpdateQuestion::Restart { .. }));
    Some((question.title(), question.detail()))
}

/// In the card, in place of its button, rather than in a dialog.
#[gpui_kit::test]
fn restarting_to_update_asks_first_when_something_typed_would_be_lost(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing_with_an_update_ready(cx, &stack, "say:not sent yet");

    ui.click("restart-to-update");

    let (title, detail) = restart_question(&ui).unwrap();
    assert_eq!(title, "Restart to update?");
    assert_eq!(detail, "What's typed in 1 chat hasn't been sent, and restarting loses it.");
    assert!(ui.has("update-question"));
    assert!(!ui.has("restart-to-update"));
    assert!(!ui.cx.has_pending_prompt());
    ui.click("update-not-now");
    assert_eq!(restart_question(&ui), None);
    assert!(ui.has("restart-to-update"));
    assert!(!ui.has("update-problem"), "it didn't try to install");
    assert_eq!(ui.composer_text(), "say:not sent yet");
}

/// The card doesn't hold anything else up, so more can be typed while it asks, and it says so before losing it.
#[gpui_kit::test]
fn restarting_anyway_asks_again_when_more_is_typed_since_it_asked(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing_with_an_update_ready(cx, &stack, "say:not sent yet");
    ui.click("restart-to-update");
    ui.new_chat();
    ui.input("say:nor this");

    ui.click("update-go-ahead");

    let (_, detail) = restart_question(&ui).unwrap();
    assert_eq!(detail, "What's typed in 2 chats hasn't been sent, and restarting loses it.");
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

    choose(&mut ui, "Roundhouse", "Check for Updates…");

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

    choose(&mut ui, "Roundhouse", "Check for Updates…");

    assert!(ui.has("update-problem"));
    assert!(!ui.has("update-answer"));
    assert!(!ui.cx.has_pending_prompt());
}
