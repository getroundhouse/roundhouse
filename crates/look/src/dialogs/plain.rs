//! A dialog that's only what's in it, which draws all of itself, such as the new workspace's composer or an image opened
//! from a chat, over the window it dims, with the one shadow a dialog has.

use std::rc::Rc;

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::dialog::{Dialog as KitDialog, DialogClose};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement as _, Pixels, Styled as _, Window, px, transparent_black};

use crate::shape::{Corner, Shaped as _};

/// What confirming does, told the window, and whether the dialog closes after it.
type Confirm = Rc<dyn Fn(&mut Window, &mut App) -> bool>;

/// What's done once the dialog closes.
type Closed = Rc<dyn Fn(&mut Window, &mut App)>;

/// A dialog that's only `shown`, `width` wide unless the window is narrower, which draws its own fill, edge and a
/// card's corners, since it's what's in it that says how it looks. It's opened with [`open_plain_dialog`].
pub fn plain_dialog(shown: impl IntoElement, width: Pixels) -> PlainDialog {
    PlainDialog {
        shown: shown.into_any_element(),
        width,
        centered: false,
        click_closes: false,
        on_confirm: None,
        on_close: None,
    }
}

/// Opens the dialog `build` makes over the window, built again each time it's drawn, as [`crate::open_dialog`] does.
/// Escape and a click outside it close it, and Enter confirms it.
pub fn open_plain_dialog(
    window: &mut Window,
    cx: &mut App,
    build: impl Fn(&mut Window, &mut App) -> PlainDialog + 'static,
) {
    window.open_dialog(cx, move |surface, window, cx| build(window, cx).drawn(surface));
}

/// A dialog that's only what's in it, made with [`plain_dialog`].
#[must_use]
pub struct PlainDialog {
    shown: AnyElement,
    width: Pixels,
    centered: bool,
    click_closes: bool,
    on_confirm: Option<Confirm>,
    on_close: Option<Closed>,
}

impl PlainDialog {
    /// In the middle of the window, rather than near its top, as an image that's opened is.
    pub fn centered(mut self) -> Self {
        self.centered = true;
        self
    }

    /// Closed by a click on it, as by one outside it, as an image that's opened is, since there's nothing in it to
    /// press.
    pub fn click_closes(mut self) -> Self {
        self.click_closes = true;
        self
    }

    /// What Enter does, which says whether it closes then, as the new workspace's composer creates and keeps it open
    /// until it can act. Without one, Enter closes it.
    pub fn on_confirm(mut self, confirm: impl Fn(&mut Window, &mut App) -> bool + 'static) -> Self {
        self.on_confirm = Some(Rc::new(confirm));
        self
    }

    /// What's done once someone closes it, however they do. Nothing is done when the app closes it itself, with
    /// [`crate::close_dialog`].
    pub fn on_close(mut self, closed: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(closed));
        self
    }

    /// The kit's dialog, with none of its own padding, fill, edge or least height, so it's what's in it.
    fn drawn(self, surface: KitDialog) -> KitDialog {
        let (on_confirm, on_close) = (self.on_confirm, self.on_close);
        surface
            .p_0()
            .min_h(px(0.))
            .w(self.width)
            .close_button(false)
            .bg(transparent_black())
            .border_0()
            .corners(Corner::Card)
            .when(self.centered, KitDialog::centered)
            .map(|surface| {
                if self.click_closes {
                    surface.child(DialogClose::new().child(self.shown))
                } else {
                    surface.child(self.shown)
                }
            })
            .on_ok(move |_, window, cx| on_confirm.as_ref().is_none_or(|confirm| confirm(window, cx)))
            .when_some(on_close, |surface, closed| surface.on_close(move |_, window, cx| closed(window, cx)))
    }
}
