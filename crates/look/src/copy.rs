//! The button that copies some text, with the pointing hand over it.

use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::{Sizable as _, Size};
use gpui_kit::{App, Div, ElementId, ParentElement as _, SharedString, Styled as _, div};

use crate::{Corner, EDGE, Hand as _, Shaped as _, colors};

/// The kit's size whose square a Copy button is, a small button's, as a test holds it.
pub(crate) const KIT_SIZE: Size = Size::Small;

/// A button that copies `value`, which its tooltip and assistive apps call `label`: a small button's square, since it's
/// set beside text, such as a turn's time or a command. The kit's copy button takes no style, only the kit's size of
/// the button inside it, so it's given a small square's. It keeps the arrow, so the hand is asked for by something
/// drawn over it. That takes no press of its own, and the button under it still does. Its pane-coloured cutout reaches
/// one edge width past the button without taking layout space, so the same treatment fits within existing rows.
pub fn copy_button(id: impl Into<ElementId>, value: impl Into<SharedString>, label: &'static str, cx: &App) -> Div {
    let copy = Clipboard::new(id).value(value).tooltip(label).accessibility_label(label);
    let copy = copy.with_size(KIT_SIZE);
    let cutout = div().absolute().inset(-EDGE).corners(Corner::Control).bg(colors(cx).pane);
    div().relative().child(cutout).child(div().opacity(0.8).child(copy)).child(div().absolute().inset_0().hand())
}
