//! Asking before what would stop or lose something, and telling what went wrong, in the app's own dialogs over its
//! window, never in the system's alert, so each looks and answers like the rest of the app.

use gpui_kit::{App, SharedString, Window};

/// What a question asks, and its two answers: one that keeps things as they are, and one that acts.
pub(crate) struct Question {
    /// What the answers are found by: `{id}-keep` and `{id}-act`.
    pub(crate) id: &'static str,
    pub(crate) title: SharedString,
    pub(crate) says: SharedString,
    /// The answer that keeps things as they are, such as Cancel or Keep working, which Enter and Escape give too.
    pub(crate) keep: &'static str,
    /// The answer that acts, such as Disconnect.
    pub(crate) act: &'static str,
    /// Whether acting removes or forgets what can't be had back from where it's asked, so its answer is filled in the
    /// error's colour.
    pub(crate) removes: bool,
}

/// Asks `asked` over `window`, and does `act` once it's answered with acting.
pub(crate) fn ask(asked: Question, window: &mut Window, cx: &mut App, act: impl Fn(&mut Window, &mut App) + 'static) {
    ask_and_hear(asked, window, cx, act, |_, _| {});
}

/// Asks `asked` over `window`, does `act` once it's answered with acting, and then `closed` however it's answered.
pub(crate) fn ask_and_hear(
    asked: Question,
    window: &mut Window,
    cx: &mut App,
    act: impl Fn(&mut Window, &mut App) + 'static,
    closed: impl Fn(&mut Window, &mut App) + 'static,
) {
    let (act, closed) = (std::rc::Rc::new(act), std::rc::Rc::new(closed));
    look::open_dialog(window, cx, move |_, _| {
        let (act, closed) = (act.clone(), closed.clone());
        let kind = if asked.removes { look::ButtonKind::Removes } else { look::ButtonKind::Main };
        let id = asked.id;
        look::dialog(asked.title.clone())
            .lead(asked.says.clone())
            .closes(look::button(format!("{id}-keep"), look::ButtonKind::Edged, look::ControlSize::Large, asked.keep))
            .confirms(look::button(format!("{id}-act"), kind, look::ControlSize::Large, asked.act))
            .careful()
            .on_confirm(move |window, cx| {
                act(window, cx);
                true
            })
            .on_close(move |window, cx| closed(window, cx))
    });
}

/// Asks `asked` over the window in front, as [`ask`] does over the one it's given, for what has no window to ask over.
pub(crate) fn ask_in_front(asked: Question, cx: &mut App, act: impl Fn(&mut Window, &mut App) + 'static) {
    let Some(window) = cx.active_window().or_else(|| cx.windows().first().copied()) else { return };
    cx.defer(move |cx| {
        let _ = window.update(cx, |_, window, cx| ask(asked, window, cx, act));
    });
}

/// Tells `title` and what it `says` over the window in front, with OK to close it, such as what went wrong where
/// nothing on screen can say it.
pub(crate) fn tell(title: impl Into<SharedString>, says: impl Into<SharedString>, cx: &mut App) {
    let (title, says) = (title.into(), says.into());
    // The window in front, or else the app's window, since what's said can come after someone has switched apps.
    let Some(window) = cx.active_window().or_else(|| cx.windows().first().copied()) else { return };
    // Once the window is free: a menu's action arrives while its window is dispatching it, and a window can't be
    // updated from inside its own update.
    cx.defer(move |cx| {
        let _ = window.update(cx, |_, window, cx| {
            look::open_dialog(window, cx, move |_, _| {
                look::dialog(title.clone()).lead(says.clone()).confirms(look::button(
                    "told-ok",
                    look::ButtonKind::Main,
                    look::ControlSize::Large,
                    "OK",
                ))
            });
        });
    });
}
