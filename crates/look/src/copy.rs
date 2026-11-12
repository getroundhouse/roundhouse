//! The button that copies some text, with the pointing hand over it.

use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::{Div, ElementId, ParentElement as _, SharedString, Styled as _, div};

use crate::Hand as _;

/// A button that copies `value`, which its tooltip and assistive apps call `label`. The kit's copy button keeps the
/// arrow and takes no style, so the hand is asked for by something drawn over it. That takes no press of its own, and
/// the button under it still does.
pub fn copy_button(id: impl Into<ElementId>, value: impl Into<SharedString>, label: &'static str) -> Div {
    let copy = Clipboard::new(id).value(value).tooltip(label).accessibility_label(label);
    div().relative().child(copy).child(div().absolute().inset_0().hand())
}
