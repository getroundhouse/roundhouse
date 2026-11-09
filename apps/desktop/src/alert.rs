//! Tells the person something in the system's own alert, over the app's window, with one button to close it.

use gpui_kit::{App, PromptLevel};

pub(crate) fn show(level: PromptLevel, message: String, detail: String, cx: &mut App) {
    // The window in front, or else the app's window, since what's said can come after someone has switched apps.
    let Some(window) = cx.active_window().or_else(|| cx.windows().first().copied()) else { return };
    // Once the window is free: a menu's action arrives while its window is dispatching it, and a window can't be
    // updated from inside its own update.
    cx.defer(move |cx| {
        let _ = window.update(cx, |_, window, cx| {
            // Nothing waits for the answer, since the one button only closes it.
            drop(window.prompt(level, &message, Some(&detail), &["OK"], cx));
        });
    });
}
