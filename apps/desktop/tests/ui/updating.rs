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

/// Chooses an item in the app menu, as the menu bar does, and returns the alert it shows.
fn choose(ui: &mut Ui<'_>, name: &str) -> (String, String) {
    let action = action(ui, name);
    ui.cx.update(|cx| cx.dispatch_action(action.as_ref()));
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

    let (message, detail) = choose(&mut ui, "About Roundhouse");

    assert_eq!(message, "Roundhouse");
    assert_eq!(detail, format!("Version {}", protocol::BUILD_VERSION));
}

#[gpui_kit::test]
fn checking_for_updates_in_a_build_that_doesnt_update_says_so(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);

    let (message, _) = choose(&mut ui, "Check for Updates…");

    assert_eq!(message, "This build doesn't update itself");
}

#[gpui_kit::test]
fn checking_for_updates_where_the_app_cant_update_says_why(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    let problem = Problem { kind: Kind::Translocated, detail: "/private/var/folders/AppTranslocation".to_string() };
    ui.cx.update(|cx| desktop::updates::show_problem(problem, cx));

    let (message, _) = choose(&mut ui, "Check for Updates…");

    assert_eq!(message, "Move Roundhouse to Applications to update it");
}
