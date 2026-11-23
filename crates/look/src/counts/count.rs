use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use written_counts::whole;

use super::CountKind;
use crate::baseline::Placed;
use crate::colors::colors;
use crate::typography::{Type, Typeset};

/// A count of `kind` that says `how_many`, whole, in a detail's text.
pub fn count(kind: CountKind, how_many: usize) -> Count {
    Count { kind, how_many, said: None, placed: None }
}

/// How many, which [`count`] makes.
#[derive(IntoElement)]
#[must_use]
pub struct Count {
    kind: CountKind,
    how_many: usize,
    said: Option<(ElementId, SharedString)>,
    placed: Option<Placed>,
}

impl Count {
    /// Says what it counts to assistive apps, as `said`, such as `3 workspaces need a look`, since the number alone
    /// says nothing, with `id`, which a test finds it by.
    pub fn said(mut self, id: impl Into<ElementId>, said: impl Into<SharedString>) -> Self {
        self.said = Some((id.into(), said.into()));
        self
    }

    /// On the baseline of the line it's in, beside text of another size, such as a tab's name.
    pub fn placed(mut self, placed: &Placed) -> Self {
        self.placed = Some(placed.clone());
        self
    }
}

impl Count {
    /// What it says, as it's written: whole, with a comma between each three figures.
    pub(super) fn written(&self) -> String {
        whole(self.how_many)
    }
}

impl RenderOnce for Count {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (color, stronger) = self.kind.drawn(colors(cx));
        let drawn = div()
            .flex_none()
            .typeset(Type::Detail)
            .when(stronger, Typeset::emphasis)
            .text_color(color)
            .child(self.written());
        let drawn = match self.placed {
            Some(placed) => placed.set(drawn),
            None => drawn,
        };
        match self.said {
            Some((id, said)) => drawn.id(id).role(Role::Status).aria_label(said).test_support().into_any_element(),
            None => drawn.into_any_element(),
        }
    }
}
