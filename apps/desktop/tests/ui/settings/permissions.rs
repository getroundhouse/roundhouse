//! Settings' Permissions page, which says how each system permission stands, as the system says, and whose buttons
//! have the system ask or open System Settings, against a stand-in for the system that a test sets.

#![cfg(test)]

use desktop::{Permission, Standing};
use gpui_kit::TestAppContext;

use super::made_up::in_a_workspace;
use super::open_settings;
use crate::support::Ui;

/// Has the system say each permission stands as given, and the app read them again, as it does when its window comes
/// in front, such as from System Settings.
fn system_says(ui: &mut Ui<'_>, standings: &[(Permission, Standing)]) {
    ui.cx.update(|cx| {
        for &(permission, standing) in standings {
            desktop::set_permission(permission, standing, cx);
        }
    });
    ui.send_to_back();
    ui.bring_to_front();
}

/// How many rows say each standing: Allowed, Not asked yet, Off, then Can't tell.
fn saying(ui: &mut Ui<'_>) -> [usize; 4] {
    ["Allowed", "Not asked yet", "Off", "Can't tell"].map(|said| ui.labelled(said).len())
}

/// The button of the permission whose row's parts are known by `id`.
fn button(id: &str) -> String {
    format!("settings-permission-{id}")
}

/// One walk through the page, each step a case named in its assertion.
#[gpui_kit::test]
fn each_permission_says_how_it_stands_and_its_button_asks_or_opens_system_settings(cx: &mut TestAppContext) {
    let (_hub, mut ui) = in_a_workspace(cx);
    system_says(
        &mut ui,
        &[
            (Permission::Notifications, Standing::Allowed),
            (Permission::FullDiskAccess, Standing::Off),
            (Permission::ScreenRecording, Standing::Off),
            (Permission::Microphone, Standing::NotAsked),
            (Permission::InputMonitoring, Standing::Unknown),
        ],
    );
    open_settings(&mut ui);
    ui.click("settings-page-permissions");

    assert_eq!(saying(&mut ui), [1, 1, 2, 1], "each row says how the system says it stands");
    assert!(!ui.has(button("notifications")), "one that's allowed has no button");
    assert!(!ui.has(button("input-monitoring")), "nor one the system won't say of");

    ui.click(button("microphone"));
    assert_eq!(ui.cx.read(desktop::permissions_asked_for), [Permission::Microphone], "Allow… has the system ask");
    assert!(ui.cx.read(desktop::permissions_opened).is_empty(), "and opens nothing");

    ui.click(button("full-disk-access"));
    let opened = ui.cx.read(desktop::permissions_opened);
    assert_eq!(opened, [Permission::FullDiskAccess], "Open System Settings opens its place there");

    system_says(&mut ui, &[(Permission::FullDiskAccess, Standing::Allowed)]);
    assert_eq!(saying(&mut ui), [2, 1, 1, 1], "coming back to the app reads it again");
    assert!(!ui.has(button("full-disk-access")), "and the button goes once it's allowed");
}
