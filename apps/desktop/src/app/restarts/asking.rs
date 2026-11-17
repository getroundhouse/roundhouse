//! The dialog that asks before restarting the older hub now, which stops what's running on it.

use std::rc::Rc;

use gpui_kit::{App, Window};

use super::UpdateQuestion;

/// Opens the dialog over the window, saying what `question` does. `restart` runs once it's confirmed, and `closed`
/// when someone closes it, with either button, Escape or a click outside it, after `restart` if it was confirmed.
/// Nothing runs when the app closes it itself.
pub(super) fn open(
    question: &UpdateQuestion,
    window: &mut Window,
    cx: &mut App,
    restart: impl Fn(&mut App) + 'static,
    closed: impl Fn(&mut App) + 'static,
) {
    let (title, detail) = (question.title(), question.detail());
    let (restart, closed) = (Rc::new(restart), Rc::new(closed));
    look::open_dialog(window, cx, move |_, _| {
        let (restart, closed) = (restart.clone(), closed.clone());
        look::dialog(title)
            .lead(detail.clone())
            .closes(look::button("restart-now-not-now", look::ButtonKind::Edged, look::ControlSize::Large, "Not now"))
            .confirms(look::button(
                "restart-now-confirm",
                look::ButtonKind::Main,
                look::ControlSize::Large,
                "Restart now",
            ))
            .on_confirm(move |_, cx| {
                restart(cx);
                true
            })
            .on_close(move |_, cx| closed(cx))
    });
}
