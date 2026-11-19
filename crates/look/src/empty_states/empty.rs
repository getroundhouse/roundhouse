use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _,
    TestSupportExt as _, Window, div,
};

use super::EmptyKind;
use super::kinds::{MARK, SAYS};
use crate::colors::colors;
use crate::controls::Button;
use crate::mark::still;
use crate::space::Space;
use crate::typography::{Type, Typeset as _};

/// An empty pane of `kind`, under `heading`, which says what's missing or what went wrong, with nothing else yet.
pub fn empty(kind: EmptyKind, heading: impl Into<SharedString>) -> Empty {
    Empty { kind, heading: heading.into(), says: Vec::new(), answers: Vec::new(), id: None }
}

/// What fills a pane with nothing in it, which [`empty`] makes: the mark, in its kind's colour, a heading, what's said
/// under it, muted, and the buttons that do something about it, in a line, all in the pane's middle.
#[derive(IntoElement)]
#[must_use]
pub struct Empty {
    kind: EmptyKind,
    heading: SharedString,
    says: Vec<SharedString>,
    answers: Vec<Button>,
    id: Option<ElementId>,
}

impl Empty {
    /// A line or two under its heading, muted, such as why. Each is a paragraph of its own.
    pub fn says(mut self, words: impl Into<SharedString>) -> Self {
        self.says.push(words.into());
        self
    }

    /// A button that does something about it, after the ones before it, such as the main action, last.
    pub fn answer(mut self, button: Button) -> Self {
        self.answers.push(button);
        self
    }

    /// Found by a test as `id`.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl RenderOnce for Empty {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let said = |words: SharedString| {
            div().max_w(SAYS).text_center().typeset(Type::Body).text_color(colors.text_muted).child(words)
        };
        let drawn = v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap(Space::Large)
            .child(still(MARK, self.kind.mark(colors)))
            .child(
                v_flex()
                    .items_center()
                    .gap(Space::Small)
                    .child(div().typeset(Type::Heading).text_color(colors.text).child(self.heading))
                    .children(self.says.into_iter().map(said)),
            )
            .child(h_flex().gap(Space::Small).children(self.answers));
        match self.id {
            Some(id) => drawn.id(id).test_support().into_any_element(),
            None => drawn.into_any_element(),
        }
    }
}
