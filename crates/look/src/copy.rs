//! The button that copies some text, with the pointing hand over it, and the button shown only while the pointer is
//! over what it copies.

use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::{Sizable as _, Size};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, Div, ElementId, InteractiveElement as _, ParentElement as _, SharedString, Styled, div};

use crate::{Corner, EDGE, Hand as _, Input, Shaped as _, colors, tooltip_words};

/// The kit's size whose square a Copy button is, a small button's, as a test holds it.
pub(crate) const KIT_SIZE: Size = Size::Small;

/// A button that copies `value`, which its tooltip and assistive apps call `label`: a small button's square, since it's
/// set beside text, such as a turn's time or a command. The kit's copy button takes no style, only the kit's size of
/// the button inside it, so it's given a small square's. It keeps the arrow, so the hand is asked for by something
/// drawn over it. That takes no press of its own, and the button under it still does. Its pane-coloured cutout reaches
/// one edge width past the button without taking layout space, so the same treatment fits within existing rows. Its
/// tooltip shows only with a pointer, as every tooltip's does.
pub fn copy_button(id: impl Into<ElementId>, value: impl Into<SharedString>, label: &'static str, cx: &App) -> Div {
    let copy = Clipboard::new(id).value(value).accessibility_label(label);
    let copy = copy.with_size(KIT_SIZE).when_some(tooltip_words(label, cx), Clipboard::tooltip);
    let cutout = div().absolute().inset(-EDGE).corners(Corner::Control).bg(colors(cx).pane);
    div().relative().child(cutout).child(div().opacity(0.8).child(copy)).child(div().absolute().inset_0().hand())
}

/// A [`copy_button`] shown only while the pointer is over `group`, or over the button itself, so a button that reaches
/// past the group stays while the pointer moves onto it, such as a prompt's. Under touch nothing rests on what it
/// copies, so there's none, and a long press copies it instead, with
/// [`crate::RightClickMenu::copied_by_a_long_press`], as `docs/spec/components.md` says.
pub fn copy_button_shown_while_over(
    group: impl Into<SharedString>,
    id: impl Into<ElementId>,
    value: impl Into<SharedString>,
    label: &'static str,
    cx: &App,
) -> Option<Div> {
    let group = group.into();
    (Input::of(cx) == Input::Pointer).then(|| {
        copy_button(id, value, label, cx).invisible().group_hover(group, Styled::visible).hover(Styled::visible)
    })
}
