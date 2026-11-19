#![cfg(test)]

#[cfg(not(target_os = "macos"))]
use crate::support::Stack;
use crate::support::{FakeHub, Ui};
use client_core::fake_hub::World;
use gpui_kit::component::input::{
    Copy, Cut, DeleteToPreviousWordStart, MoveToPreviousWord, Paste, Redo, SelectAll, Undo,
};
use gpui_kit::{
    Action, AppContext as _, KeyContext, Keystroke, OperatingSystem, OwnedMenu, OwnedMenuItem, TestAppContext,
};
use protocol::model::{SessionStatus, WorkspaceId};

/// Where the items a Mac has in its app menu are on the system the tests run on, as the menu and the item: in the
/// app menu on a Mac, and elsewhere in File, or in Help.
pub(crate) const ABOUT: (&str, &str) =
    if ON_A_MAC { ("Roundhouse", "About Roundhouse") } else { ("Help", "About Roundhouse") };
pub(crate) const SETTINGS: (&str, &str) =
    if ON_A_MAC { ("Roundhouse", "Settings…") } else { ("File", "Settings…") };
pub(crate) const CHECK_FOR_UPDATES: (&str, &str) =
    if ON_A_MAC { ("Roundhouse", "Check for Updates…") } else { ("Help", "Check for Updates…") };
pub(crate) const QUIT: (&str, &str) = if ON_A_MAC {
    ("Roundhouse", "Quit Roundhouse")
} else if cfg!(target_os = "windows") {
    ("File", "Exit")
} else {
    ("File", "Quit")
};
const ON_A_MAC: bool = cfg!(target_os = "macos");

/// A menu from the menu bar, by the name it's given.
fn menu(ui: &Ui<'_>, name: &str) -> OwnedMenu {
    let menus = ui.cx.update(|cx| cx.get_menus()).unwrap();
    menus.into_iter().find(|menu| menu.name == name).unwrap_or_else(|| panic!("no {name} menu"))
}

/// A menu's items as macOS lists them, with `-` for each separator.
pub(crate) fn items(ui: &Ui<'_>, name: &str) -> Vec<String> {
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
pub(super) fn action(ui: &Ui<'_>, menu_name: &str, name: &str) -> Box<dyn Action> {
    let action = menu(ui, menu_name).items.into_iter().find_map(|item| match item {
        OwnedMenuItem::Action { name: shown, action, .. } if shown == name => Some(action),
        _ => None,
    });
    action.unwrap_or_else(|| panic!("no {name} in the {menu_name} menu"))
}

/// The key shown beside an item, as [`keyed`] writes it.
pub(super) fn key(ui: &Ui<'_>, menu_name: &str, name: &str) -> Option<String> {
    key_for(ui, action(ui, menu_name, name).as_ref())
}

/// The key shown beside an item that does `action`: on a Mac the first bound to it, which macOS shows, and elsewhere
/// the one the kit shows, which is the last bound that works where focus is, or else the last that works anywhere.
fn key_for(ui: &Ui<'_>, action: &dyn Action) -> Option<String> {
    let os = ui.cx.update(|cx| cx.platform_info().os);
    let binding = if os == OperatingSystem::Mac {
        let keymap = ui.cx.update(|cx| cx.key_bindings());
        keymap.borrow().bindings_for_action(action).next().cloned()
    } else {
        let found = |window: &gpui_kit::Window| {
            window
                .highest_precedence_binding_for_action(action)
                .or_else(|| window.highest_precedence_binding_for_action_in_context(action, KeyContext::default()))
        };
        ui.cx.update(|cx| ui.handle.update(cx, |_, window, _| found(window))).unwrap()
    }?;
    let keystrokes = binding.keystrokes().iter().map(|keystroke| keystroke.unparse(os));
    Some(keystrokes.collect::<Vec<_>>().join(" "))
}

/// `keys`, written as they're bound, as [`key`] gives them on the system the tests run on, so `secondary-n` is `cmd-n`
/// on a Mac and `ctrl-n` elsewhere.
pub(super) fn keyed(ui: &Ui<'_>, keys: &str) -> String {
    let os = ui.cx.update(|cx| cx.platform_info().os);
    Keystroke::parse_with_os(keys, os).unwrap().unparse(os)
}

/// What pressing a key would do where focus is, without doing it, or `None` for nothing.
pub(crate) fn bound_where_focus_is(ui: &mut Ui<'_>, key: &str) -> Option<Box<dyn Action>> {
    ui.frame();
    let os = ui.cx.update(|cx| cx.platform_info().os);
    let keystroke = Keystroke::parse_with_os(key, os).unwrap();
    let keymap = ui.cx.update(|cx| cx.key_bindings());
    let contexts = ui.cx.update_window(ui.handle, |_, window, _| window.context_stack()).unwrap();
    let (bindings, _) = keymap.borrow().bindings_for_input(&[keystroke], &contexts);
    bindings.first().map(|binding| binding.action().boxed_clone())
}

/// Every key bound to something where focus is, by itself rather than after another, with what pressing it would do
/// there.
pub(crate) fn every_key_bound_where_focus_is(ui: &mut Ui<'_>) -> Vec<(Keystroke, Box<dyn Action>)> {
    ui.frame();
    let keymap = ui.cx.update(|cx| cx.key_bindings());
    let contexts = ui.cx.update_window(ui.handle, |_, window, _| window.context_stack()).unwrap();
    let keymap = keymap.borrow();
    let mut keys: Vec<Keystroke> = Vec::new();
    for binding in keymap.bindings() {
        if let [only] = binding.keystrokes()
            && !keys.contains(only.inner())
        {
            keys.push(only.inner().clone());
        }
    }
    let bound = |keys: Keystroke| {
        let (bindings, _) = keymap.bindings_for_input(std::slice::from_ref(&keys), &contexts);
        bindings.first().map(|binding| (keys, binding.action().boxed_clone()))
    };
    keys.into_iter().filter_map(bound).collect()
}

/// Whether macOS shows the item as one that can be chosen, which is whether anything where focus is takes its action.
pub(super) fn enabled(ui: &mut Ui<'_>, menu_name: &str, name: &str) -> bool {
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

/// Opens the app on a repository with no workspace, so there's no composer, for the menus the app draws off the Mac,
/// whose items reach a real hub.
#[cfg(not(target_os = "macos"))]
pub(super) fn with_a_repository<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui
}

/// A window on a fake hub holding `world`. What the menus hold, which of their items can act, and what the Edit and
/// View items do, are the app's alone, so they ask nothing of a hub. The hub goes with the test, so it's returned to
/// be kept.
pub(super) fn on(cx: &mut TestAppContext, world: World) -> (FakeHub, Ui<'_>) {
    let fake = FakeHub::holding(world);
    let ui = Ui::open_on(cx, &fake);
    (fake, ui)
}

/// A window on a repository with one workspace, showing, and its chat, with nothing in it. `world` has what else the
/// test needs, such as the workspace's diff.
pub(super) fn in_a_workspace(
    cx: &mut TestAppContext,
    world: impl FnOnce(&mut World, &WorkspaceId),
) -> (FakeHub, Ui<'_>) {
    let mut made = World::default();
    let repo = made.repo("signals");
    let workspace = made.workspace(&repo, "ledger");
    made.chat(&workspace, SessionStatus::Idle, Vec::new());
    world(&mut made, &workspace);
    let (fake, mut ui) = on(cx, made);
    ui.wait("the workspace's chat", |ui| ui.view().selected() == Some(&workspace) && ui.transcript().is_some());
    (fake, ui)
}

/// Each menu this system has, in order, with its items, `-` for each separator. Off the Mac, what a Mac has in its app
/// menu ends File, as `menus::tests` checks for each system, so File's are only those it starts with.
/// macOS adds Enter Full Screen below View's last separator, which only a Mac has, and puts its own items above
/// Window's. The window has no workspace, so View offers to show the diff, and to hide the sidebar, which shows.
fn menus_and_their_items() -> Vec<(&'static str, Vec<&'static str>)> {
    let file =
        vec!["New Workspace", "New Chat", "New Terminal", "Add Project…", "-", "Close Chat", "Archive Workspace"];
    let edit = vec!["Undo", "Redo", "-", "Cut", "Copy", "Paste", "Select All", "-", "Find"];
    let mut view = vec![
        "Hide Sidebar",
        "Show Diff",
        "-",
        "Back",
        "Forward",
        "Previous Workspace",
        "Next Workspace",
        "-",
        "Previous Tab",
        "Next Tab",
    ];
    if !ON_A_MAC {
        return vec![
            ("File", file),
            ("Edit", edit),
            ("View", view),
            ("Help", vec!["Check for Updates…", "-", "About Roundhouse"]),
        ];
    }
    view.push("-");
    let app_menu = vec![
        "About Roundhouse",
        "Check for Updates…",
        "-",
        "Settings…",
        "Services",
        "-",
        "Hide Roundhouse",
        "Hide Others",
        "Show All",
        "-",
        "Quit Roundhouse",
    ];
    vec![
        ("Roundhouse", app_menu),
        ("File", file),
        ("Edit", edit),
        ("View", view),
        ("Window", vec!["-", "Minimize", "Zoom"]),
    ]
}

/// Each item of the window's own, by its menu and its name, with what it does and the key it shows, as [`keyed`]
/// writes it, or `None` for one with no key.
fn items_with_their_actions_and_keys() -> Vec<(&'static str, &'static str, &'static dyn Action, Option<&'static str>)> {
    let mut rows: Vec<(&str, &str, &dyn Action, Option<&str>)> = vec![
        ("File", "New Workspace", &desktop::NewWorkspace, Some("secondary-n")),
        ("File", "New Chat", &desktop::NewChat, Some("secondary-t")),
        ("File", "New Terminal", &desktop::NewTerminal, Some("secondary-shift-t")),
        ("File", "Add Project…", &desktop::AddRepository, None),
        ("File", "Close Chat", &desktop::CloseChat, Some("secondary-w")),
        ("File", "Archive Workspace", &desktop::ArchiveWorkspace, Some("secondary-shift-a")),
        ("View", "Hide Sidebar", &desktop::ToggleSidebar, Some("secondary-b")),
        ("View", "Show Diff", &desktop::ToggleDiff, Some("secondary-shift-b")),
        ("View", "Back", &desktop::GoBack, Some("secondary-[")),
        ("View", "Forward", &desktop::GoForward, Some("secondary-]")),
        ("View", "Previous Workspace", &desktop::PreviousWorkspace, Some("alt-secondary-up")),
        ("View", "Next Workspace", &desktop::NextWorkspace, Some("alt-secondary-down")),
        ("View", "Previous Tab", &desktop::PreviousChat, Some(if ON_A_MAC { "secondary-{" } else { "ctrl-shift-tab" })),
        ("View", "Next Tab", &desktop::NextChat, Some(if ON_A_MAC { "secondary-}" } else { "ctrl-tab" })),
    ];
    if ON_A_MAC {
        rows.extend([
            ("Roundhouse", "Hide Roundhouse", &desktop::Hide as &dyn Action, Some("secondary-h")),
            ("Roundhouse", "Hide Others", &desktop::HideOthers, Some("alt-secondary-h")),
            ("Roundhouse", "Show All", &desktop::ShowAll, None),
            ("Roundhouse", "Quit Roundhouse", &desktop::Quit, Some("secondary-q")),
            ("Roundhouse", "Settings…", &desktop::OpenSettings, Some("secondary-,")),
            ("Window", "Minimize", &desktop::Minimize, Some("secondary-m")),
            ("Window", "Zoom", &desktop::Zoom, None),
        ]);
    }
    rows
}

/// The Edit menu's items are the kit's own actions, so each does what its key does in a text box. A Mac shows the
/// kit's keys beside them. Elsewhere the kit binds other keys, and there's no menu bar to show them.
fn the_edit_menu_has_the_text_boxs_actions(ui: &Ui<'_>) {
    let kit: [(&str, &dyn Action, &str); 6] = [
        ("Undo", &Undo, "cmd-z"),
        ("Redo", &Redo, "cmd-shift-z"),
        ("Cut", &Cut, "cmd-x"),
        ("Copy", &Copy, "cmd-c"),
        ("Paste", &Paste, "cmd-v"),
        ("Select All", &SelectAll, "cmd-a"),
    ];
    for (name, kits, shown) in kit {
        assert!(action(ui, "Edit", name).partial_eq(kits), "{name} isn't the kit's");
        if ON_A_MAC {
            assert_eq!(key(ui, "Edit", name).as_deref(), Some(shown), "{name}");
        }
    }
}

/// Chooses About, and reads the alert it opens.
fn about_says_which_build_this_is(ui: &mut Ui<'_>) {
    choose(ui, ABOUT.0, ABOUT.1);

    ui.wait("About's alert", |ui| ui.asked().is_some());
    let (message, detail) = ui.asked().unwrap();
    assert_eq!(message, "Roundhouse", "About's alert");
    assert_eq!(detail, format!("Version {}", client_core::fake_hub::BUILD), "About says which build this is");
}

/// Off the Mac, the Edit menu's keys are the kit's, which the next test checks where focus is.
#[gpui_kit::test]
fn every_menu_has_its_items_each_with_its_action_and_key_and_about_says_which_build_this_is(cx: &mut TestAppContext) {
    let (_hub, mut ui) = on(cx, World::default());

    let menus = menus_and_their_items();
    let names: Vec<String> = ui.cx.update(|cx| cx.get_menus()).unwrap().iter().map(|m| m.name.to_string()).collect();
    assert_eq!(names, menus.iter().map(|(name, _)| *name).collect::<Vec<_>>(), "the menus this system has");
    for (menu, wanted) in menus {
        let shown = items(&ui, menu);
        let shown = if menu == "File" { &shown[..wanted.len()] } else { &shown[..] };
        assert_eq!(shown, wanted, "{menu}'s items, in order");
    }
    for (menu, item, does, keys) in items_with_their_actions_and_keys() {
        assert!(action(&ui, menu, item).partial_eq(does), "{menu}, {item}: what it does");
        assert_eq!(key(&ui, menu, item), keys.map(|keys| keyed(&ui, keys)), "{menu}, {item}: its key");
    }
    the_edit_menu_has_the_text_boxs_actions(&ui);
    has_find_for_the_chat_showing(&ui);
    about_says_which_build_this_is(&mut ui);
}

/// Find has a menu of its own under the text box's items, as a Mac app has it, and finds in the chat showing. What it
/// does is tested by pressing its keys, in `finding.rs`.
fn has_find_for_the_chat_showing(ui: &Ui<'_>) {
    let Some(OwnedMenuItem::Submenu(find)) = menu(ui, "Edit").items.into_iter().last() else {
        panic!("Find isn't a menu at the end of Edit")
    };
    let shown: Vec<(String, Box<dyn Action>)> = find
        .items
        .into_iter()
        .filter_map(|item| match item {
            OwnedMenuItem::Action { name, action, .. } => Some((name, action)),
            _ => None,
        })
        .collect();
    let wanted: [(&str, &dyn Action, &str); 3] = [
        ("Find…", &desktop::FindInChat, "secondary-f"),
        ("Find Next", &desktop::FindNext, "secondary-g"),
        ("Find Previous", &desktop::FindPrevious, "secondary-shift-g"),
    ];
    assert_eq!(shown.len(), wanted.len());
    for ((name, action), (wanted_name, wanted_action, wanted_key)) in shown.iter().zip(wanted) {
        assert_eq!(name, wanted_name);
        assert!(action.partial_eq(wanted_action), "{name}");
        assert_eq!(key_for(ui, action.as_ref()), Some(keyed(ui, wanted_key)), "{name}");
    }
}

/// Whether Find can be chosen, which is whether there's a chat showing to find in.
pub(super) fn can_find(ui: &mut Ui<'_>) -> bool {
    ui.frame();
    ui.cx.update(|cx| cx.is_action_available(&desktop::FindInChat))
}

/// A text box binds keys of its own, and none of the app's may take them, nor may they take the app's and the
/// window's. Off the Mac, Cmd is the Windows or Super key, which the system keeps, so what only a Mac does has no key
/// there, and the others are on Ctrl, as the text box's editing keys are. Choosing the Edit menu's items, rather than
/// pressing their keys, which the kit's own tests cover, edits the composer. Find works in the chat showing, unless
/// the subagent tab is in front, which isn't searched.
#[gpui_kit::test]
fn in_the_composer_its_keys_are_its_own_and_the_edit_menu_edits_it(cx: &mut TestAppContext) {
    let (_hub, mut ui) = in_a_workspace(cx, |_, _| {});
    ui.input("say:hello");
    assert!(enabled(&mut ui, "Edit", "Paste") && enabled(&mut ui, "Edit", "Select All"), "with a text box");
    assert!(can_find(&mut ui), "with a chat showing");

    let keys: [(&str, &dyn Action, bool); 4] = [
        ("secondary-h", &desktop::Hide, ON_A_MAC),
        ("alt-secondary-h", &desktop::HideOthers, ON_A_MAC),
        ("secondary-q", &desktop::Quit, true),
        ("secondary-m", &desktop::Minimize, ON_A_MAC),
    ];
    for (key, item, bound) in keys {
        let pressed = bound_where_focus_is(&mut ui, key);
        let what = format!("the app's {key}, {}, while typing", item.name());
        assert_eq!(pressed.is_some_and(|pressed| pressed.partial_eq(item)), bound, "{what}");
    }
    if !ON_A_MAC {
        assert!(bound_where_focus_is(&mut ui, "cmd-q").is_none(), "the Windows or Super key quits nothing");
    }
    let mut edits: Vec<(&str, &dyn Action)> = vec![
        ("secondary-a", &SelectAll),
        ("secondary-c", &Copy),
        ("secondary-x", &Cut),
        ("secondary-v", &Paste),
        ("secondary-z", &Undo),
    ];
    // Keys only the text box off the Mac has, which an app-wide key could take there without a Mac's tests seeing.
    if !ON_A_MAC {
        edits.extend([
            ("ctrl-y", &Redo as &dyn Action),
            ("ctrl-left", &MoveToPreviousWord),
            ("ctrl-backspace", &DeleteToPreviousWordStart),
        ]);
    }
    for (key, edit) in edits {
        let pressed = bound_where_focus_is(&mut ui, key);
        assert!(pressed.is_some_and(|pressed| pressed.partial_eq(edit)), "{key} isn't the text box's");
    }

    choose(&mut ui, "Edit", "Select All");
    choose(&mut ui, "Edit", "Cut");
    assert_eq!(ui.composer_text(), "", "Cut, after Select All");
    choose(&mut ui, "Edit", "Paste");
    choose(&mut ui, "Edit", "Paste");
    assert_eq!(ui.composer_text(), "say:hellosay:hello", "pasted twice");
    choose(&mut ui, "Edit", "Undo");
    assert_eq!(ui.composer_text(), "say:hello", "the second paste undone");

    let (pane, transcript) = (ui.pane().unwrap(), ui.transcript().unwrap());
    let chat = ui.cx.read_entity(&transcript, |t, _| t.session_id().clone());
    ui.cx.update(|cx| pane.update(cx, |p, cx| p.open_subagent_tab(chat, "call-1".into(), "Look around".into(), cx)));
    assert!(!can_find(&mut ui), "with the subagent tab in front, which isn't searched");
}
