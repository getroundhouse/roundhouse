use gpui_kit::{App, Hsla, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _, Window, div, hsla};

use super::badge::HEIGHT;
use crate::space::Space;
use crate::typography::{Type, Typeset as _};

/// A label that says `words`, tinted in `color`, the one whoever made it gave it, such as a GitHub label's.
pub fn label(words: impl Into<SharedString>, color: Hsla) -> Label {
    Label { words: words.into(), color }
}

/// A tag someone chose, which [`label`] makes.
#[derive(IntoElement)]
#[must_use]
pub struct Label {
    words: SharedString,
    color: Hsla,
}

impl RenderOnce for Label {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let words = readable(self.color);
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(HEIGHT)
            .px(Space::Small)
            // Round, since its colour is someone else's rather than one of the app's.
            .rounded_full()
            .border_1()
            .border_color(words.opacity(0.32))
            .bg(words.opacity(0.14))
            .typeset(Type::Detail)
            .text_color(words)
            .child(self.words)
    }
}

/// `color`'s hue, as light as a label's words are on the pane whatever the colour was, so a pale yellow and a dark red
/// read alike, with its saturation held where a very saturated hue would glare.
pub(super) fn readable(color: Hsla) -> Hsla {
    hsla(color.h, color.s.min(0.75), color.l.clamp(0.72, 0.82), 1.)
}
