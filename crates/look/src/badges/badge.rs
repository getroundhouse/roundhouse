use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};

use super::BadgeKind;
use crate::colors::colors;
use crate::shape::{Corner, Shaped as _};
use crate::space::Space;
use crate::typography::{Type, Typeset as _};

/// How tall a badge or a label is: smaller than a pill or a chip, so it reads as said of what it's beside.
pub(crate) const HEIGHT: Pixels = px(18.);

/// A badge of `kind` that says `words`.
pub fn badge(kind: BadgeKind, words: impl Into<SharedString>) -> Badge {
    Badge { kind, words: words.into(), id: None }
}

/// What's true of the thing beside it, which [`badge`] makes.
#[derive(IntoElement)]
#[must_use]
pub struct Badge {
    kind: BadgeKind,
    words: SharedString,
    id: Option<ElementId>,
}

impl Badge {
    /// Its ID, which a test finds it by, and which tells assistive apps what it says.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl RenderOnce for Badge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (words, fill) = self.kind.colors(colors(cx));
        let drawn = div()
            .flex()
            .flex_none()
            .items_center()
            .h(HEIGHT)
            .px(Space::Small)
            .corners(Corner::Control)
            .bg(fill)
            .typeset(Type::Detail)
            .emphasis()
            .text_color(words)
            .child(self.words.clone());
        match self.id {
            Some(id) => drawn.id(id).aria_label(self.words).test_support().into_any_element(),
            None => drawn.into_any_element(),
        }
    }
}
