//! The badge on the app's icon in the Dock, which says how many workspaces have a chat that needs the person, so it
//! still shows from another app once a notification has gone. Only a Mac shows one yet.

use gpui_kit::{App, Global};

/// What shows the badge. Only the app as it runs has one, so a test or a picture of the app leaves the Dock alone.
pub(crate) struct Tile;

impl Global for Tile {}

/// How many workspaces the badge says, as last shown.
struct Shown(usize);

impl Global for Shown {}

/// Shows `workspaces` on the badge, or no badge for none. The app's one window shows one hub at a time, so the badge
/// is that hub's. Nothing is asked of the Dock when the badge already says it.
pub(crate) fn show_badge(workspaces: usize, cx: &mut App) {
    if cx.try_global::<Shown>().is_some_and(|shown| shown.0 == workspaces) {
        return;
    }
    cx.set_global(Shown(workspaces));
    if cx.has_global::<Tile>() {
        badge(workspaces);
    }
}

/// How many workspaces the badge says, whether or not there was a Dock to show it, for a test to read.
#[cfg(any(test, feature = "testing"))]
pub fn says(cx: &App) -> usize {
    cx.try_global::<Shown>().map_or(0, |shown| shown.0)
}

#[cfg(target_os = "macos")]
fn badge(workspaces: usize) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    // AppKit is only for the main thread, which is where the app's views run, so this always has it.
    let Some(main) = MainThreadMarker::new() else {
        return;
    };
    let label = (workspaces > 0).then(|| NSString::from_str(&workspaces.to_string()));
    NSApplication::sharedApplication(main).dockTile().setBadgeLabel(label.as_deref());
}

/// Nothing elsewhere yet, so on Windows and Linux only the sidebar says which workspaces need the person.
#[cfg(not(target_os = "macos"))]
fn badge(_workspaces: usize) {}
