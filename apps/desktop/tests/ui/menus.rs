//! The menu bar: what each menu holds, the key beside each item, and what the items do. Hiding, minimizing and
//! zooming happen in macOS, which a headless window doesn't have, so for those the tests stop at the action.

#![cfg(test)]

use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use gpui_kit::{Action, AppContext as _, Keystroke, OperatingSystem, OwnedMenu, OwnedMenuItem, TestAppContext};

use crate::support::{Stack, Ui};

mod file_and_view;

/// A menu from the menu bar, by the name it's given.
fn menu(ui: &Ui<'_>, name: &str) -> OwnedMenu {
    let menus = ui.cx.update(|cx| cx.get_menus()).unwrap();
    menus.into_iter().find(|menu| menu.name == name).unwrap_or_else(|| panic!("no {name} menu"))
}

/// A menu's items as macOS lists them, with `-` for each separator.
fn items(ui: &Ui<'_>, name: &str) -> Vec<String> {
    let items = menu(ui, name).items.into_iter();
    items
        .map(|item| match item {
            OwnedMenuItem::Separator => "-".to_string(),
            OwnedMenuItem::Action { name, .. } => name,
            OwnedMenuItem::Submenu(menu) => menu.name.to_string(),
            OwnedMenuItem::SystemMenu(menu) => menu.name.to_string(),
        })
        .collect()
}

/// What choosing an item does.
fn action(ui: &Ui<'_>, menu_name: &str, name: &str) -> Box<dyn Action> {
    let action = menu(ui, menu_name).items.into_iter().find_map(|item| match item {
        OwnedMenuItem::Action { name: shown, action, .. } if shown == name => Some(action),
        _ => None,
    });
    action.unwrap_or_else(|| panic!("no {name} in the {menu_name} menu"))
}

/// The key macOS shows beside an item, which is the first one bound to its action.
fn key(ui: &Ui<'_>, menu_name: &str, name: &str) -> Option<String> {
    let action = action(ui, menu_name, name);
    let keymap = ui.cx.update(|cx| cx.key_bindings());
    let keymap = keymap.borrow();
    let binding = keymap.bindings_for_action(action.as_ref()).next()?;
    let keystrokes = binding.keystrokes().iter().map(|keystroke| keystroke.unparse(OperatingSystem::Mac));
    Some(keystrokes.collect::<Vec<_>>().join(" "))
}

/// What pressing a key would do where focus is, without doing it.
fn bound_where_focus_is(ui: &mut Ui<'_>, key: &'static str) -> Box<dyn Action> {
    ui.frame();
    let keystroke = Keystroke::parse(key).unwrap();
    let keymap = ui.cx.update(|cx| cx.key_bindings());
    let contexts = ui.cx.update_window(ui.handle, |_, window, _| window.context_stack()).unwrap();
    let (bindings, _) = keymap.borrow().bindings_for_input(&[keystroke], &contexts);
    bindings.first().unwrap_or_else(|| panic!("nothing takes {key} here")).action().boxed_clone()
}

/// Whether macOS shows the item as one that can be chosen, which is whether anything where focus is takes its action.
fn enabled(ui: &mut Ui<'_>, menu_name: &str, name: &str) -> bool {
    let action = action(ui, menu_name, name);
    ui.frame();
    ui.cx.update(|cx| cx.is_action_available(action.as_ref()))
}

/// Chooses an item, as the menu bar does.
pub(crate) fn choose(ui: &mut Ui<'_>, menu_name: &str, name: &str) {
    let action = action(ui, menu_name, name);
    ui.cx.update(|cx| cx.dispatch_action(action.as_ref()));
    ui.frame();
}

/// Opens the app on a repository with no workspace, so there's no composer.
fn with_a_repository<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui
}

/// Opens a workspace, so its composer has focus, with this typed in it.
fn typing<'a>(cx: &'a mut TestAppContext, stack: &Stack, typed: &str) -> Ui<'a> {
    let mut ui = with_a_repository(cx, stack);
    ui.new_workspace();
    ui.input(typed);
    ui
}

#[gpui_kit::test]
fn the_app_menu_has_what_every_mac_app_has_in_the_same_order(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);

    assert_eq!(
        items(&ui, "Roundhouse"),
        [
            "About Roundhouse",
            "-",
            "Check for Updates…",
            "-",
            "Services",
            "-",
            "Hide Roundhouse",
            "Hide Others",
            "Show All",
            "-",
            "Quit Roundhouse",
        ]
    );
    assert!(action(&ui, "Roundhouse", "Hide Roundhouse").partial_eq(&desktop::Hide));
    assert!(action(&ui, "Roundhouse", "Hide Others").partial_eq(&desktop::HideOthers));
    assert!(action(&ui, "Roundhouse", "Show All").partial_eq(&desktop::ShowAll));
    assert!(action(&ui, "Roundhouse", "Quit Roundhouse").partial_eq(&desktop::Quit));
    assert_eq!(key(&ui, "Roundhouse", "Hide Roundhouse").as_deref(), Some("cmd-h"));
    assert_eq!(key(&ui, "Roundhouse", "Hide Others").as_deref(), Some("alt-cmd-h"));
    assert_eq!(key(&ui, "Roundhouse", "Show All"), None);
    assert_eq!(key(&ui, "Roundhouse", "Quit Roundhouse").as_deref(), Some("cmd-q"));
}

#[gpui_kit::test]
fn the_window_menu_has_minimize_and_zoom(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);

    // macOS puts its own items above these.
    assert_eq!(items(&ui, "Window"), ["-", "Minimize", "Zoom"]);
    assert!(action(&ui, "Window", "Minimize").partial_eq(&desktop::Minimize));
    assert!(action(&ui, "Window", "Zoom").partial_eq(&desktop::Zoom));
    assert_eq!(key(&ui, "Window", "Minimize").as_deref(), Some("cmd-m"));
    assert_eq!(key(&ui, "Window", "Zoom"), None);
}

/// A text box binds keys of its own, and none of them may take these.
#[gpui_kit::test]
fn the_app_and_window_keys_work_while_typing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing(cx, &stack, "say:not sent yet");

    for (key, menu_name, name) in [
        ("cmd-h", "Roundhouse", "Hide Roundhouse"),
        ("alt-cmd-h", "Roundhouse", "Hide Others"),
        ("cmd-q", "Roundhouse", "Quit Roundhouse"),
        ("cmd-m", "Window", "Minimize"),
    ] {
        let pressed = bound_where_focus_is(&mut ui, key);
        assert!(pressed.partial_eq(action(&ui, menu_name, name).as_ref()), "{key} isn't {name} in the composer");
    }
}

/// The kit's own actions, so each item does what its key does in a text box.
#[gpui_kit::test]
fn the_edit_menu_has_the_text_box_keys(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);

    assert_eq!(items(&ui, "Edit"), ["Undo", "Redo", "-", "Cut", "Copy", "Paste", "Select All"]);
    let kit: [(&str, &dyn Action, &str); 6] = [
        ("Undo", &Undo, "cmd-z"),
        ("Redo", &Redo, "cmd-shift-z"),
        ("Cut", &Cut, "cmd-x"),
        ("Copy", &Copy, "cmd-c"),
        ("Paste", &Paste, "cmd-v"),
        ("Select All", &SelectAll, "cmd-a"),
    ];
    for (name, kits, shown) in kit {
        assert!(action(&ui, "Edit", name).partial_eq(kits), "{name} isn't the kit's");
        // Elsewhere the kit binds other keys, and there's no menu bar to show them.
        if cfg!(target_os = "macos") {
            assert_eq!(key(&ui, "Edit", name).as_deref(), Some(shown), "{name}");
        }
    }
}

/// Choosing the items, rather than pressing their keys, which the kit's own tests cover.
#[gpui_kit::test]
fn the_edit_menu_edits_the_composer(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing(cx, &stack, "say:hello");

    choose(&mut ui, "Edit", "Select All");
    choose(&mut ui, "Edit", "Cut");
    assert_eq!(ui.composer_text(), "");
    choose(&mut ui, "Edit", "Paste");
    choose(&mut ui, "Edit", "Paste");
    assert_eq!(ui.composer_text(), "say:hellosay:hello");
    choose(&mut ui, "Edit", "Undo");

    assert_eq!(ui.composer_text(), "say:hello");
}

/// macOS greys out an item when nothing where focus is would take its action.
#[gpui_kit::test]
fn the_edit_menu_is_greyed_out_where_theres_no_text_box(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    assert!(!enabled(&mut ui, "Edit", "Paste"));
    assert!(!enabled(&mut ui, "Edit", "Select All"));
    // Text selected in a chat copies, wherever focus is.
    assert!(enabled(&mut ui, "Edit", "Copy"));

    ui.new_workspace();

    assert!(enabled(&mut ui, "Edit", "Paste"));
    assert!(enabled(&mut ui, "Edit", "Select All"));
}

#[gpui_kit::test]
fn about_says_which_build_this_is(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);

    choose(&mut ui, "Roundhouse", "About Roundhouse");

    ui.wait("the alert", |ui| ui.cx.has_pending_prompt());
    let (message, detail) = ui.cx.pending_prompt().unwrap();
    assert_eq!(message, "Roundhouse");
    assert_eq!(detail, format!("Version {}", protocol::BUILD_VERSION));
}
