//! The badge on the app's icon in the Dock, which says how many workspaces have a chat that needs the person, so it
//! still shows from another app once a notification has gone. Only a Mac shows one yet.

use gpui_kit::{App, Global};

use crate::permissions::Badges;

/// What shows the badge. Only the app as it runs has one, so a test or a picture of the app leaves the Dock alone.
pub(crate) struct Tile;

impl Global for Tile {}

/// How many workspaces the badge says, as last shown.
struct Shown(usize);

impl Global for Shown {}

/// Where asking a Mac for the badge has got.
#[derive(Default)]
struct Asked {
    /// It shows the badge, or never will, so nothing more is asked.
    settled: bool,
    /// An ask hasn't been answered yet, so another waits for it.
    asking: bool,
}

impl Global for Asked {}

/// Shows `workspaces` on the badge, or no badge for none. The app's one window shows one hub at a time, so the badge
/// is that hub's. Nothing is asked of the Dock when the badge already says it, but a Mac is asked for the badge
/// whenever there's one to show, until it shows them.
pub(crate) fn show_badge(workspaces: usize, cx: &mut App) {
    if !cx.has_global::<Tile>() {
        cx.set_global(Shown(workspaces));
        return;
    }
    if cx.try_global::<Shown>().is_none_or(|shown| shown.0 != workspaces) {
        cx.set_global(Shown(workspaces));
        badge(workspaces);
    }
    if workspaces > 0 {
        make_sure_it_shows(cx);
    }
}

/// Asks for the badge with notifications, which a Mac needs before it shows one, and shows the badge again once it's
/// granted, since a Mac drops one it may not show. GPUI's notifications ask for none, and once notifications are
/// allowed a Mac grants the badge without asking the person. Until they're allowed, it's asked again at the next look,
/// one ask at a time, since the person may allow them at any moment.
fn make_sure_it_shows(cx: &mut App) {
    let asked = cx.default_global::<Asked>();
    if asked.settled || asked.asking {
        return;
    }
    asked.asking = true;
    crate::permissions::ask_for_badges(cx, |badges, cx| {
        let asked = cx.default_global::<Asked>();
        asked.asking = false;
        asked.settled = badges != Badges::NotYet;
        if badges == Badges::Granted {
            badge(cx.try_global::<Shown>().map_or(0, |shown| shown.0));
        }
    });
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
