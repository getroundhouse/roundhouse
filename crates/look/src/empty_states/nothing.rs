use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _,
    TestSupportExt as _, Window, div,
};

use crate::colors::colors;
use crate::space::Space;
use crate::typography::{Type, Typeset as _};

/// How far in from the sides of what it's in a line saying there's nothing to show keeps, wherever it is.
pub(super) const ROOM: Space = Space::XLarge;

/// A line saying there's nothing to show, such as `No changes yet`.
pub fn nothing_to_show(words: impl Into<SharedString>) -> NothingToShow {
    NothingToShow { words: words.into(), id: None }
}

/// One muted line in the middle of what it fills, saying there's nothing to show, or not yet, where nothing needs
/// doing about it, which [`nothing_to_show`] makes, such as a search that matched nothing or a file that's empty.
#[derive(IntoElement)]
#[must_use]
pub struct NothingToShow {
    words: SharedString,
    id: Option<ElementId>,
}

impl NothingToShow {
    /// Found by a test as `id`.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl RenderOnce for NothingToShow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        // It grows to fill a column it's in, and fills anything else, so its line is in the middle of the room.
        let drawn = div()
            .flex_1()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .px(ROOM)
            .text_center()
            .typeset(Type::Body)
            .text_color(colors(cx).text_muted)
            .child(self.words);
        match self.id {
            Some(id) => drawn.id(id).test_support().into_any_element(),
            None => drawn.into_any_element(),
        }
    }
}
