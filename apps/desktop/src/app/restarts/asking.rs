//! The dialog that asks before restarting the older hub now, which stops what's running on it.

use std::rc::Rc;

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::{App, ParentElement as _, Styled as _, Window, div};

use look::{Hand as _, Typeset as _};

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
    window.open_dialog(cx, move |dialog, _, cx| {
        let (restart, closed) = (restart.clone(), closed.clone());
        let detail = div().typeset(look::Type::Body).text_color(look::colors(cx).text_muted).child(detail.clone());
        dialog
            .title(title)
            // Its two buttons are the ways out of it.
            .close_button(false)
            .child(detail)
            .footer(
                DialogFooter::new()
                    .child(DialogClose::new().child(Button::new("restart-now-not-now").hand().label("Not now")))
                    .child(
                        DialogAction::new()
                            .child(Button::new("restart-now-confirm").hand().primary().label("Restart now")),
                    ),
            )
            .on_ok(move |_, _, cx| {
                restart(cx);
                true
            })
            .on_close(move |_, _, cx| closed(cx))
    });
}
