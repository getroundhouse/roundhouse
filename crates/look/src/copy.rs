//! The button that copies some text, with the pointing hand over it.

use gpui_kit::component::Sizable as _;
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::{Div, ElementId, ParentElement as _, SharedString, Styled as _, div};

use crate::{ControlSize, Hand as _};

/// A button that copies `value`, which its tooltip and assistive apps call `label`: a small button's square, since it's
/// set beside text, such as a turn's time or a command. The kit's copy button takes no style, only the kit's size of
/// the button inside it, so it's given a small square's. It keeps the arrow, so the hand is asked for by something
/// drawn over it. That takes no press of its own, and the button under it still does.
pub fn copy_button(id: impl Into<ElementId>, value: impl Into<SharedString>, label: &'static str) -> Div {
    let copy = Clipboard::new(id).value(value).tooltip(label).accessibility_label(label);
    let copy = copy.with_size(ControlSize::Small.kit_icon());
    div().relative().child(copy).child(div().absolute().inset_0().hand())
}
