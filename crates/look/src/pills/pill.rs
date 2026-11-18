use gpui_kit::{AnyElement, App, IntoElement, ParentElement as _, Pixels, RenderOnce, Styled as _, Window, div, px};

use super::PillKind;
use crate::colors::colors;
use crate::shape::{Corner, Shaped as _};
use crate::space::Space;
use crate::typography::{Type, Typeset as _};

/// How tall a pill is, which sits in a line of body text with a little room above and below its words, as tall as a
/// small control.
pub(crate) const HEIGHT: Pixels = px(24.);

/// A pill of `kind` that shows `value`, which is its words, or its words with what's marked in them, such as what a
/// find matched. It's cut short with an ellipsis where it has no room.
pub fn pill(kind: PillKind, value: impl IntoElement) -> Pill {
    Pill { kind, value: value.into_any_element() }
}

/// A value as it was written, which [`pill`] makes.
#[derive(IntoElement)]
#[must_use]
pub struct Pill {
    kind: PillKind,
    value: AnyElement,
}

impl RenderOnce for Pill {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (words, fill) = self.kind.colors(colors(cx));
        div()
            .flex()
            .items_center()
            .min_w_0()
            .h(HEIGHT)
            .px(Space::Small)
            .corners(Corner::Control)
            .bg(fill)
            .typeset(Type::Code)
            .text_color(words)
            .child(div().min_w_0().truncate().child(self.value))
    }
}
