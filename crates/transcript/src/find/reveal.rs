//! Bringing the match being shown into view. Whatever draws it says where it was drawn, after laying it out, and that
//! goes two ways: an area in a row that scrolls what it holds, such as a call's output, scrolls itself to it, and the
//! transcript scrolls its list to it, so it's below the bar. Each frame says where it's drawn now, so the two settle in
//! a frame or two.
//!
//! Markdown, which the transcript draws from the kit's blocks, says where a match is as plain text does.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{Bounds, ElementId, IntoElement, Pixels, Styled as _, TextLayout, canvas, px};

/// How much room is left above the match when something scrolls to it, so it isn't on the very edge.
pub(crate) const ROOM: Pixels = px(48.);

/// Where the match being shown was last drawn, in one transcript.
#[derive(Clone, Default)]
pub(crate) struct Reveal(Rc<RefCell<Slot>>);

#[derive(Default)]
struct Slot {
    /// Whether the match is to be brought into view.
    wanted: bool,
    drawn: Option<Drawn>,
    /// What of the transcript's list is in view, less what the bar covers, as it was last drawn, for a test to see
    /// whether the match ended up in it.
    #[cfg(feature = "testing")]
    visible: Option<Bounds<Pixels>>,
    /// Where it was last drawn, wanted or not, for a test to see where it ended up.
    #[cfg(feature = "testing")]
    painted: Option<Bounds<Pixels>>,
}

#[derive(Clone, Debug)]
pub(crate) struct Drawn {
    /// The area it's in that scrolls, if it's in one.
    pub(crate) area: Option<ElementId>,
    /// Where it was drawn, in the window.
    pub(crate) bounds: Bounds<Pixels>,
}

impl std::fmt::Debug for Reveal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let slot = self.0.borrow();
        f.debug_struct("Reveal").field("wanted", &slot.wanted).field("drawn", &slot.drawn).finish_non_exhaustive()
    }
}

impl Reveal {
    /// Asks for the match being shown to be brought into view, forgetting where the last one was.
    pub(crate) fn want(&self) {
        let mut slot = self.0.borrow_mut();
        slot.wanted = true;
        slot.drawn = None;
        #[cfg(feature = "testing")]
        {
            slot.painted = None;
        }
    }

    /// Where it was drawn, while it's wanted.
    pub(crate) fn drawn(&self) -> Option<Drawn> {
        let slot = self.0.borrow();
        slot.drawn.clone().filter(|_| slot.wanted)
    }

    pub(crate) fn is_wanted(&self) -> bool {
        self.0.borrow().wanted
    }

    /// It's in view, or nothing more can be done.
    pub(crate) fn done(&self) {
        let mut slot = self.0.borrow_mut();
        slot.wanted = false;
        slot.drawn = None;
    }

    /// What it's drawn in has scrolled, so where it was drawn is out of date until it's drawn again.
    pub(crate) fn moved(&self) {
        self.0.borrow_mut().drawn = None;
    }

    /// Says what of the list is in view now, less what the bar covers.
    #[cfg(feature = "testing")]
    pub(crate) fn set_visible(&self, visible: Bounds<Pixels>) {
        self.0.borrow_mut().visible = Some(visible);
    }

    /// Says the match was drawn at `bounds`, in `area` if it's in an area that scrolls, while it's wanted.
    pub(crate) fn drawn_at(&self, bounds: Bounds<Pixels>, area: Option<ElementId>) {
        let mut slot = self.0.borrow_mut();
        #[cfg(feature = "testing")]
        {
            slot.painted = Some(bounds);
        }
        if slot.wanted {
            slot.drawn = Some(Drawn { area, bounds });
        }
    }

    /// Whether the match was last drawn in view below the bar, once it has been drawn since it was wanted.
    #[cfg(feature = "testing")]
    pub(crate) fn shows_in_view(&self) -> Option<bool> {
        let slot = self.0.borrow();
        let (painted, visible) = (slot.painted?, slot.visible?);
        Some(painted.top() >= visible.top() && painted.bottom() <= visible.bottom())
    }
}

/// Draws nothing, and says where the match at `start` in the text `layout` lays out was drawn, once it has been.
/// `area` is the area it's in that scrolls, if it's in one.
pub(crate) fn here(layout: TextLayout, start: usize, area: Option<ElementId>, reveal: Reveal) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |_, (), _, _| {
            let Some(at) = layout.position_for_index(start) else { return };
            reveal.drawn_at(Bounds::new(at, gpui_kit::size(px(1.), layout.line_height())), area.clone());
        },
    )
    .absolute()
    .size_0()
}

/// How far an area showing `visible` would scroll down to bring `bounds` into view with [`ROOM`] above it, if it
/// isn't in view: up is negative.
pub(crate) fn scroll_by(visible: Bounds<Pixels>, bounds: Bounds<Pixels>) -> Option<Pixels> {
    let inside = bounds.top() >= visible.top() && bounds.bottom() <= visible.bottom();
    (!inside).then(|| bounds.top() - visible.top() - ROOM.min(visible.size.height / 4.))
}

/// How far an area showing `visible` would scroll right to bring `bounds` into view with [`ROOM`] before it, if it
/// isn't in view: left is negative.
pub(crate) fn scroll_across(visible: Bounds<Pixels>, bounds: Bounds<Pixels>) -> Option<Pixels> {
    let inside = bounds.left() >= visible.left() && bounds.right() <= visible.right();
    (!inside).then(|| bounds.left() - visible.left() - ROOM.min(visible.size.width / 4.))
}
