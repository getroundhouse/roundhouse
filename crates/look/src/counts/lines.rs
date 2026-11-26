use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use written_counts::whole;

use super::LinesKind;
use crate::baseline::Placed;
use crate::colors::{Strength, colors};
use crate::space::Space;
use crate::typography::Typeset;

/// How strongly lines last read some time ago are drawn, rather than kept current: enough of their colours to tell
/// green from red, faint enough that they don't read as now.
const AS_LAST_READ: Strength = Strength::XStrong;

/// The lines added and removed of `kind`, `+12 −3`, leaving out whichever is none.
pub fn lines_changed(kind: LinesKind, added: usize, removed: usize) -> LinesChanged {
    LinesChanged { kind, added, removed, written: whole, as_last_read: false, said: None, placed: None }
}

/// How much changed, which [`lines_changed`] makes.
#[derive(IntoElement)]
#[must_use]
pub struct LinesChanged {
    kind: LinesKind,
    added: usize,
    removed: usize,
    written: fn(usize) -> String,
    as_last_read: bool,
    said: Option<(ElementId, SharedString)>,
    placed: Option<Placed>,
}

impl LinesChanged {
    /// Writes each count as `write` does, such as `written_counts::shortened`, where there's room for only a few
    /// figures. Each is whole otherwise, as `written_counts::whole` writes it.
    pub fn written_as(mut self, write: fn(usize) -> String) -> Self {
        self.written = write;
        self
    }

    /// Drawn fainter, as lines read some time ago rather than kept current, so they don't read as now.
    pub fn as_last_read(mut self, as_last_read: bool) -> Self {
        self.as_last_read = as_last_read;
        self
    }

    /// Says what they are to assistive apps, as `said`, such as `12 lines added and 3 removed`, with `id`, which a test
    /// finds them by.
    pub fn said(mut self, id: impl Into<ElementId>, said: impl Into<SharedString>) -> Self {
        self.said = Some((id.into(), said.into()));
        self
    }

    /// On the baseline of the line it's in, beside body text, such as a call's line.
    pub fn placed(mut self, placed: &Placed) -> Self {
        self.placed = Some(placed.clone());
        self
    }

    /// What it says, as it's written: a plus before what's added, and a minus sign, not a hyphen, before what's
    /// removed.
    pub(super) fn written(&self) -> (Option<String>, Option<String>) {
        let added = (self.added > 0).then(|| format!("+{}", (self.written)(self.added)));
        let removed = (self.removed > 0).then(|| format!("\u{2212}{}", (self.written)(self.removed)));
        (added, removed)
    }
}

impl RenderOnce for LinesChanged {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let shown = |color: Hsla| if self.as_last_read { AS_LAST_READ.of(color) } else { color };
        let (added, removed) = self.written();
        let drawn = h_flex()
            .flex_none()
            .gap(Space::XSmall)
            .typeset_code(cx)
            .when(self.kind.stronger(), Typeset::emphasis)
            .when_some(added, |d, added| d.child(div().text_color(shown(colors.added)).child(added)))
            .when_some(removed, |d, removed| d.child(div().text_color(shown(colors.removed)).child(removed)));
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
