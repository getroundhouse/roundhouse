//! What an element that scrolls a selection's texts, such as the chat's list or a document's, does for it in each
//! frame: before it lays the texts out, it ends a drag it wasn't drawn for and scrolls on under a pointer held past its
//! top or bottom, and after, once the texts have said where they were drawn, it moves a drag's far end to what's under
//! the pointer.

use gpui_kit::{App, Bounds, GlobalElementId, Pixels, Window};

use super::Selection;

/// What the element that scrolls a selection's texts is to do as a frame starts.
pub(crate) struct Started {
    /// Whether it was drawn in the last frame, as it wasn't when another tab or workspace was shown.
    pub(crate) drawn_last_frame: bool,
    /// How far to scroll it down for a drag held past its top or bottom, if it is. Up is less than nothing.
    pub(crate) scroll_by: Option<Pixels>,
}

impl Selection {
    /// Starts a frame of the element known as `id` that scrolls the texts, in `bounds`, before it lays them out. A drag
    /// ends as the button comes up, which is only heard while the element is drawn, so one it wasn't drawn for in the
    /// last frame is over. One held past its top or bottom has the frame after asked for, so it scrolls on.
    pub(crate) fn frame_started(
        &self,
        id: Option<&GlobalElementId>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &App,
    ) -> Started {
        let drawn_last_frame =
            id.is_some_and(|id| window.with_element_state(id, |was: Option<()>, _| (was, ())).is_some());
        if !drawn_last_frame {
            self.release();
        }
        let scroll_by = self.scroll_past_edge(bounds, cx.background_executor().now());
        if scroll_by.is_some() {
            window.request_animation_frame();
        }
        Started { drawn_last_frame, scroll_by }
    }

    /// After the texts in `bounds` are laid out and have said where they were drawn: a drag's far end follows what's
    /// under the pointer, so it keeps up as they move under a pointer held still, with the frame after asked for if
    /// that changed what's selected.
    pub(crate) fn texts_laid_out(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        if self.follow(bounds) {
            window.request_animation_frame();
        }
    }
}
