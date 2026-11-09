//! The menu bar, and what its items do that no view does. Each item is an action, so macOS shows its key beside it.

use gpui_kit::{App, Menu, MenuItem, PromptLevel};

use crate::{About, CheckForUpdates, Quit};

/// Sets the menu bar, and handles its items wherever focus is.
pub(crate) fn set(cx: &mut App) {
    // macOS names the first menu after the app, whatever it's called here.
    cx.set_menus([Menu::new("Roundhouse").items([
        MenuItem::action("About Roundhouse", About),
        MenuItem::separator(),
        MenuItem::action("Check for Updates…", CheckForUpdates),
        MenuItem::separator(),
        MenuItem::action("Quit", Quit),
    ])]);
    cx.on_action(|_: &About, cx| about(cx));
    cx.on_action(|_: &CheckForUpdates, cx| crate::updates::check_now(cx));
    cx.on_action(|_: &Quit, cx| cx.quit());
}

/// Says which build this is, the same as the sidebar does.
fn about(cx: &mut App) {
    let version = format!("Version {}", protocol::BUILD_VERSION);
    crate::alert::show(PromptLevel::Info, "Roundhouse".to_string(), version, cx);
}
