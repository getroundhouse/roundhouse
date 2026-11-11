//! Bringing the match being shown into view. Whatever draws it says where it was drawn, after laying it out, and that
//! goes two ways: an area in a row that scrolls what it holds, such as a call's output, scrolls itself to it, and the
//! transcript scrolls its list to it, so it's below the bar. Each frame says where it's drawn now, so the two settle in
//! a frame or two.
//!
//! Markdown is drawn by the kit's text view, which scrolls the transcript's list to a match itself, and says where it
//! is only while it's out of view. So while it's being brought into view, it's laid out as if what the bar covers were
//! out of view too, and what it says goes the same two ways.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::component::text::TextView;
use gpui_kit::{
    AnyElement, App, Bounds, ContentMask, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Styled as _, TextLayout, Window, canvas, px,
};

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
    /// What of the transcript's list is in view, less what the bar covers, as it was last drawn.
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

    /// Whether the match was last drawn in view below the bar, once it has been drawn since it was wanted. Markdown's
    /// view says where it is only while it isn't in view, so this is for a match in plain text.
    #[cfg(feature = "testing")]
    pub(crate) fn shows_in_view(&self) -> Option<bool> {
        let slot = self.0.borrow();
        let (painted, visible) = (slot.painted?, slot.visible?);
        Some(painted.top() >= visible.top() && painted.bottom() <= visible.bottom())
    }

    /// What's in view while the match is wanted.
    fn in_view(&self) -> Option<Bounds<Pixels>> {
        let slot = self.0.borrow();
        slot.visible.filter(|_| slot.wanted)
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

/// The kit's view of Markdown that has the match being shown, in `area` if it's in an area that scrolls. The view
/// brings the match into view itself, as it's asked to, laid out while it does as if what the bar covers were out of
/// view, and says where the match is while it isn't.
pub(crate) fn markdown(view: TextView, area: Option<ElementId>, reveal: &Reveal) -> AnyElement {
    let said = reveal.clone();
    let view = view.on_reveal(move |line, window, _| {
        said.drawn_at(line, area.clone());
        window.refresh();
    });
    BarredView { view: view.into_any_element(), reveal: reveal.clone() }.into_any_element()
}

/// A view laid out with what the bar covers out of view while the match is being brought into view.
struct BarredView {
    view: AnyElement,
    reveal: Reveal,
}

impl IntoElement for BarredView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BarredView {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.view.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        // Only while it's laid out, which is when the view says whether the match is in view: it's drawn whole.
        match self.reveal.in_view() {
            Some(bounds) => {
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    self.view.prepaint(window, cx);
                });
            }
            None => {
                self.view.prepaint(window, cx);
            }
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut (),
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.view.paint(window, cx);
    }
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
