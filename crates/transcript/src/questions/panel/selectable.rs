//! Text the panel shows to be selected and copied, such as the command a tool's use would run. The panel is in the
//! composer's place, not in the chat's list, so it's painted whenever it shows, and the kit's selectable text, whose
//! selection is the window's and keeps what's selected only while it's painted, can select it.

use std::rc::Rc;

use gpui_kit::base::{self, TextSelectionHandle};
use gpui_kit::{App, ElementId, IntoElement, RenderOnce, SharedString, Window};

/// The kit's selectable text, drawn again as a drag goes over it, since what a drag changes is the selection, and
/// nothing that's drawn.
#[derive(IntoElement)]
pub(super) struct SelectableText {
    /// Keeps its part in the selection from one frame to the next, so it's this text's alone.
    id: ElementId,
    text: SharedString,
}

impl SelectableText {
    pub(super) fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self { id: id.into(), text: text.into() }
    }
}

impl RenderOnce for SelectableText {
    #[expect(
        clippy::disallowed_methods,
        reason = "the panel is in the composer's place, not the chat's list, so it's painted whenever it shows, and \
                  the window's selection keeps what's selected of it"
    )]
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let text = self.text.clone();
        let kept = window.use_keyed_state((self.id.clone(), "selection"), cx, |window, cx| {
            let handle = TextSelectionHandle::new(text, cx);
            let redraws = Rc::new(handle.refresh_window_on_change(window, cx));
            (handle, redraws)
        });
        let handle = kept.read(cx).0.clone();
        base::SelectableText::with_handle(self.id, handle, self.text)
    }
}
