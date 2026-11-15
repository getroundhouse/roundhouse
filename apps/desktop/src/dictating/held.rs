//! Holding Space to dictate. `Option-Space` starts at once, as a binding where a prompt is typed, as `keys.rs` binds
//! it, and plain Space once it's held long enough in the composer, as `workspace_pane/holding_space.rs` has it. Letting
//! go of Space is heard around everything the window draws, by `app::HearsTheKeyLetGo`, before anything else hears it,
//! wherever focus has gone by then, even into a dialog. Once it's held to dictate, until it's let go of, Space's repeats
//! are swallowed for every window, before anything hears them, since they'd type spaces, even once `Option` is let go of
//! first.

use gpui_kit::{App, Global, KeyUpEvent, KeystrokeEvent, Window};

/// The key held to dictate at once, beside plain Space, which waits: Option-Space on a Mac, which types nothing anyone
/// needs in a prompt, a non-breaking space. Cmd-Space is Spotlight's, Ctrl-Space switches input sources, and Fn alone
/// is the system's own dictation. *Unconfirmed.*
pub(crate) const HOLD_TO_DICTATE_KEY: &str = "alt-space";

/// The key held to dictate, without what's held with it, as a key event names it.
pub(crate) const SPACE: &str = "space";

/// Whether Space is held to dictate, from when a key took it to, whether or not a recording started, as when the
/// system asks first whether the app may use the microphone, until it's let go of.
#[derive(Default)]
struct HeldToDictate(bool);

impl Global for HeldToDictate {}

/// A key has been taken to dictate, `Option-Space` or Space held long enough, and Space stays down until [`let_go`] or
/// [`window_left`].
pub(crate) fn held(cx: &mut App) {
    cx.default_global::<HeldToDictate>().0 = true;
}

/// Whether `event` lets go of Space, which stops a recording it started, and ends its being held towards one. Space
/// types again from then on.
pub(crate) fn let_go(event: &KeyUpEvent, cx: &mut App) -> bool {
    if event.keystroke.key != SPACE {
        return false;
    }
    if held_to_dictate(cx) {
        cx.default_global::<HeldToDictate>().0 = false;
    }
    true
}

/// Another app has come to the front, where Space is let go of without this one hearing.
pub(crate) fn window_left(cx: &mut App) {
    if held_to_dictate(cx) {
        cx.default_global::<HeldToDictate>().0 = false;
    }
}

fn held_to_dictate(cx: &App) -> bool {
    cx.try_global::<HeldToDictate>().is_some_and(|held| held.0)
}

/// Keeps Space from typing while it's held to dictate, from before anything else hears it.
pub(crate) fn swallow_held_spaces(cx: &mut App) {
    cx.intercept_keystrokes(|event: &KeystrokeEvent, _: &mut Window, cx: &mut App| {
        if event.keystroke.key == SPACE && held_to_dictate(cx) {
            cx.stop_propagation();
        }
    })
    .detach();
}
