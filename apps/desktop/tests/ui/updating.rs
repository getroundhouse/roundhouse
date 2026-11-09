//! What the app shows about updating itself, and the app menu. Only an app Velopack installed updates for real, so
//! a failure here is made up, and a check finds a build that doesn't update.

#![cfg(test)]

use desktop::updates::{Kind, Problem};
use gpui_kit::{Action, OwnedMenuItem, TestAppContext};

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

#[gpui_kit::test]
fn restarting_to_update_asks_first_when_something_typed_would_be_lost(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing_with_an_update_ready(cx, &stack, "say:not sent yet");

    ui.click("restart-to-update");
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());

    let (message, detail) = ui.cx.pending_prompt().unwrap();
    assert_eq!(message, "Restart to update?");
    assert_eq!(detail, "What's typed in 1 chat hasn't been sent, and restarting loses it.");
    ui.cx.simulate_prompt_answer("Keep working");
    ui.frame();
    assert_eq!(ui.composer_text(), "say:not sent yet");
}

/// Only an app Velopack installed can install an update, so here it says it couldn't, which shows it went ahead.
#[gpui_kit::test]
fn restarting_to_update_with_nothing_typed_asks_nothing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing_with_an_update_ready(cx, &stack, "  ");
    assert!(!ui.has("update-problem"));

    ui.click("restart-to-update");

    assert!(!ui.cx.has_pending_prompt());
    assert!(ui.has("update-problem"));
}

/// The names of the app menu's items, leaving out separators.
fn app_menu(ui: &Ui<'_>) -> Vec<String> {
    let menus = ui.cx.update(|cx| cx.get_menus()).unwrap();
    let items = menus[0].items.iter();
    items
        .filter_map(|item| if let OwnedMenuItem::Action { name, .. } = item { Some(name.clone()) } else { None })
        .collect()
}

/// What choosing an item in the app menu does.
fn action(ui: &Ui<'_>, name: &str) -> Box<dyn Action> {
    let menus = ui.cx.update(|cx| cx.get_menus()).unwrap();
    let action = menus[0].items.iter().find_map(|item| match item {
        OwnedMenuItem::Action { name: shown, action, .. } if shown == name => Some(action.boxed_clone()),
        _ => None,
    });
    action.unwrap_or_else(|| panic!("no {name} in the app menu"))
}

/// Chooses an item in the app menu, as the menu bar does.
fn choose(ui: &mut Ui<'_>, name: &str) {
    let action = action(ui, name);
    ui.cx.update(|cx| cx.dispatch_action(action.as_ref()));
    ui.frame();
}

/// Chooses an item in the app menu, and returns the alert it shows.
fn choose_and_read(ui: &mut Ui<'_>, name: &str) -> (String, String) {
    choose(ui, name);
    ui.wait("the alert", |ui| ui.cx.has_pending_prompt());
    ui.cx.pending_prompt().unwrap()
}

#[gpui_kit::test]
fn the_app_menu_has_about_check_for_updates_and_quit(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);

    assert_eq!(app_menu(&ui), ["About Roundhouse", "Check for Updates…", "Quit"]);
    // The same as ⌘Q, which the menu shows beside it.
    assert!(action(&ui, "Quit").partial_eq(&desktop::Quit));
}

#[gpui_kit::test]
fn about_says_which_build_this_is(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);

    let (message, detail) = choose_and_read(&mut ui, "About Roundhouse");

    assert_eq!(message, "Roundhouse");
    assert_eq!(detail, format!("Version {}", protocol::BUILD_VERSION));
}

/// Under the build, for a few seconds, rather than in an alert.
#[gpui_kit::test]
fn checking_for_updates_in_a_build_that_doesnt_update_says_so_in_the_sidebar(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    assert!(!ui.has("update-answer"));

    choose(&mut ui, "Check for Updates…");

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

    choose(&mut ui, "Check for Updates…");

    assert!(ui.has("update-problem"));
    assert!(!ui.has("update-answer"));
    assert!(!ui.cx.has_pending_prompt());
}
