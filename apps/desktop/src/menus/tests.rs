use std::collections::BTreeSet;

use gpui_kit::{Action, Menu, OwnedMenuItem};

use super::{Says, System, menus};
use crate::workspace_pane::Mark;
use crate::{Hide, HideOthers, Minimize, ShowAll, Zoom};

/// Every action `system`'s menus hold, by name, in submenus too.
fn actions(system: System) -> BTreeSet<&'static str> {
    fn from(items: Vec<OwnedMenuItem>, found: &mut BTreeSet<&'static str>) {
        for item in items {
            match item {
                OwnedMenuItem::Action { action, .. } => {
                    found.insert(action.name());
                }
                OwnedMenuItem::Submenu(menu) => from(menu.items, found),
                OwnedMenuItem::Separator | OwnedMenuItem::SystemMenu(_) => {}
            }
        }
    }
    let mut found = BTreeSet::new();
    for menu in menus(system, Says::default()) {
        from(menu.owned().items, &mut found);
    }
    found
}

/// Each menu's name, and its items' names with `-` for each separator.
fn arranged(system: System) -> Vec<(String, Vec<String>)> {
    arranged_saying(system, Says::default())
}

/// As [`arranged`], with the menus saying `says`.
fn arranged_saying(system: System, says: Says) -> Vec<(String, Vec<String>)> {
    let named = |item: OwnedMenuItem| match item {
        OwnedMenuItem::Separator => "-".to_string(),
        OwnedMenuItem::Action { name, .. } => name,
        OwnedMenuItem::Submenu(menu) => menu.name.to_string(),
        OwnedMenuItem::SystemMenu(menu) => menu.name.to_string(),
    };
    let menus = menus(system, says).into_iter().map(Menu::owned);
    menus.map(|menu| (menu.name.to_string(), menu.items.into_iter().map(named).collect())).collect()
}

const OFF_THE_MAC: [System; 2] = [System::Windows, System::Linux];

#[test]
fn every_item_off_the_mac_is_one_a_mac_has() {
    let on_a_mac = actions(System::Mac);

    for system in OFF_THE_MAC {
        let extra: Vec<_> = actions(system).difference(&on_a_mac).copied().collect();
        assert!(extra.is_empty(), "{system:?} has what a Mac doesn't: {extra:?}");
    }
}

#[test]
fn every_item_a_mac_has_is_off_the_mac_too_but_hiding_minimizing_and_zooming() {
    let only_a_macs = [Hide.name(), HideOthers.name(), ShowAll.name(), Minimize.name(), Zoom.name()];
    let wanted: BTreeSet<_> = actions(System::Mac).into_iter().filter(|name| !only_a_macs.contains(name)).collect();

    for system in OFF_THE_MAC {
        assert_eq!(actions(system), wanted, "{system:?}");
    }
}

/// As Windows arranges menus: no app menu, Settings… and quitting at the end of File, and a Help menu.
#[test]
fn off_the_mac_the_menus_are_arranged_as_windows_arranges_them() {
    for (system, quits) in [(System::Windows, "Exit"), (System::Linux, "Quit")] {
        let menus = arranged(system);

        let names: Vec<&str> = menus.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["File", "Edit", "View", "Help"], "{system:?}");
        let file = [
            "New Workspace",
            "New Chat",
            "New Terminal",
            "Add Project…",
            "-",
            "Close Chat",
            "Mark as Unread",
            "Archive Workspace",
            "-",
            "Settings…",
            "-",
            quits,
        ];
        assert_eq!(menus[0].1, file, "{system:?}");
        assert_eq!(menus[1].1, arranged(System::Mac)[2].1, "Edit is as a Mac has it, Find and all");
        assert_eq!(
            menus[2].1,
            [
                "Show Sidebar",
                "Show Diff",
                "-",
                "Back",
                "Forward",
                "Previous Workspace",
                "Next Workspace",
                "-",
                "Previous Tab",
                "Next Tab"
            ],
            "with nothing for macOS to add"
        );
        assert_eq!(menus[3].1, ["Check for Updates…", "-", "About Roundhouse"], "{system:?}");
    }
}

/// What every Mac app has. The window's tests check these on a Mac, with their keys, since only a Mac has them.
#[test]
fn a_mac_has_its_app_menu_and_its_window_menu() {
    let menus = arranged(System::Mac);

    let names: Vec<&str> = menus.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["Roundhouse", "File", "Edit", "View", "Window"]);
    let app_menu = [
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
    assert_eq!(menus[0].1, app_menu);
    let file = [
        "New Workspace",
        "New Chat",
        "New Terminal",
        "Add Project…",
        "-",
        "Close Chat",
        "Mark as Unread",
        "Archive Workspace",
    ];
    assert_eq!(menus[1].1, file);
    let view = [
        "Show Sidebar",
        "Show Diff",
        "-",
        "Back",
        "Forward",
        "Previous Workspace",
        "Next Workspace",
        "-",
        "Previous Tab",
        "Next Tab",
        "-",
    ];
    assert_eq!(menus[3].1, view, "for Enter Full Screen");
    assert_eq!(menus[4].1, ["-", "Minimize", "Zoom"]);
}

#[test]
fn file_says_what_marking_the_workspace_showing_does() {
    for system in [System::Mac, System::Windows, System::Linux] {
        let file = |marking| {
            let menus = arranged_saying(system, Says { marking, ..Says::default() });
            menus.into_iter().find(|(name, _)| name == "File").unwrap().1
        };
        assert!(file(Some(Mark::Read)).contains(&"Mark as Read".to_string()), "{system:?}");
        assert!(file(Some(Mark::Unread)).contains(&"Mark as Unread".to_string()), "{system:?}");
    }
}
