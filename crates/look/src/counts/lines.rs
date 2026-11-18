use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, IntoElement, ParentElement as _, RenderOnce, Styled as _, Window, div};

use crate::baseline::Placed;
use crate::colors::colors;
use crate::space::Space;
use crate::typography::{Type, Typeset as _};

/// The lines added to a file and removed from it, `+12 −3`, leaving out whichever is none.
pub fn lines_changed(added: usize, removed: usize) -> LinesChanged {
    LinesChanged { added, removed, placed: None }
}

/// How much a file changed, which [`lines_changed`] makes.
#[derive(IntoElement)]
#[must_use]
pub struct LinesChanged {
    added: usize,
    removed: usize,
    placed: Option<Placed>,
}

impl LinesChanged {
    /// On the baseline of the line it's in, beside body text, such as a call's line.
    pub fn placed(mut self, placed: &Placed) -> Self {
        self.placed = Some(placed.clone());
        self
    }

    /// What it says, as it's written: a plus before what's added, and a minus sign, not a hyphen, before what's
    /// removed.
    pub(super) fn written(&self) -> (Option<String>, Option<String>) {
        let added = (self.added > 0).then(|| format!("+{}", self.added));
        let removed = (self.removed > 0).then(|| format!("\u{2212}{}", self.removed));
        (added, removed)
    }
}

impl RenderOnce for LinesChanged {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let (added, removed) = self.written();
        let drawn = h_flex()
            .flex_none()
            .gap(Space::XSmall)
            .typeset(Type::Code)
            .when_some(added, |d, added| d.child(div().text_color(colors.added).child(added)))
            .when_some(removed, |d, removed| d.child(div().text_color(colors.removed).child(removed)));
        match self.placed {
            Some(placed) => placed.set(drawn),
            None => drawn,
        }
    }
}
