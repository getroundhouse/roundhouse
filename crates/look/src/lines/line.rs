use gpui_kit::component::h_flex;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement as _, RenderOnce, Styled as _, Window, div};

use crate::colors::colors;
use crate::shape::EDGE;
use crate::space::Space;

/// How far what's in a line's middle is from the line at each side.
pub(super) const AROUND_THE_MIDDLE: Space = Space::Medium;

/// A line across what it's in, with nothing in its middle yet.
pub fn line() -> Line {
    Line { down: false, middle: None }
}

/// A line across what it's in, or down it, which [`line`] makes, such as between the parts of a call's block, in the
/// edge's colour. One with something in its middle, such as what a compaction did, is a line on each side of it.
#[derive(IntoElement)]
#[must_use]
pub struct Line {
    down: bool,
    middle: Option<AnyElement>,
}

impl Line {
    /// Down what it's in, rather than across it, such as a guide beside a folder's files.
    pub fn down(mut self) -> Self {
        self.down = true;
        self
    }

    /// With `middle` in its middle, as a line across the chat says what happened there.
    pub fn middle(mut self, middle: impl IntoElement) -> Self {
        self.middle = Some(middle.into_any_element());
        self
    }
}

impl RenderOnce for Line {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let edge = colors(cx).edge;
        match self.middle {
            None if self.down => div().flex_none().h_full().w(EDGE).bg(edge).into_any_element(),
            None => div().flex_none().w_full().h(EDGE).bg(edge).into_any_element(),
            Some(middle) => {
                let side = || div().flex_1().h(EDGE).bg(edge);
                h_flex()
                    .w_full()
                    .items_center()
                    .gap(AROUND_THE_MIDDLE)
                    .child(side())
                    .child(middle)
                    .child(side())
                    .into_any_element()
            }
        }
    }
}
