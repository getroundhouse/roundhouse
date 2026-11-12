//! Clicking, the way a hand does.

#![cfg(test)]

use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{App, AppContext as _, ElementId, MouseButton, Pixels, Point, Window, point, px};

use super::Ui;

/// Clicks it as a hand does: the pointer comes to its middle, then moves a pixel while the button is down, as it does
/// in a quick press, though not far enough to start a drag. A click that didn't move would miss what goes wrong only
/// when the pointer moves under a press, such as a button that goes when the row it's on stops being hovered. Like
/// the kit's click, it finds it in a fresh frame and only clicks it if it can be seen, so the press can't land on
/// whatever is over it or in its place.
pub(crate) fn click(window: &mut Window, id: impl Into<ElementId>, cx: &mut App) {
    let id = id.into();
    window.render_frame(cx);
    let target = window.find(id.clone());
    assert!(target.visible(), "{id:?} can't be seen, so it can't be clicked");
    click_at(window, target.bounds().center(), cx);
}

/// Clicks at `at` as a hand does, as [`click`] says, for a place with nothing of its own to find it by, such as the
/// backdrop around a dialog.
pub(crate) fn click_at(window: &mut Window, at: Point<Pixels>, cx: &mut App) {
    window.pointer_move(at, None, cx);
    window.pointer_down(at, MouseButton::Left, cx);
    let at = at + point(px(1.), px(0.));
    window.pointer_move(at, Some(MouseButton::Left), cx);
    window.pointer_up(at, MouseButton::Left, cx);
}

impl Ui<'_> {
    /// Clicks it as a hand does, with [`click`].
    pub(crate) fn click(&mut self, id: impl Into<ElementId>) {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| click(window, id, cx)).unwrap();
        self.frame();
    }

    /// Clicks at `at` as a hand does, with [`click_at`].
    pub(crate) fn click_at(&mut self, at: Point<Pixels>) {
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| click_at(window, at, cx)).unwrap();
        self.frame();
    }
}
