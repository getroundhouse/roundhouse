//! Bringing something into view in a view of the selection's texts: the match Find is showing, in a chat, or the end of
//! what's selected that a key moved, in any of them. There's one at a time, and asking for one forgets the other, so
//! it's whichever was asked for last. Whatever draws it says where it was drawn, and that goes two ways: an area in a
//! row that scrolls what it holds, such as a call's output, scrolls itself to it as the frame paints, and says it has,
//! and what scrolls the texts, the chat's list, a document's or a page, scrolls to it as the next frame starts, as
//! `crate::selection`'s `Scrolled` has it. So the area goes first, and the two settle in a frame or two. Until it's
//! drawn, what scrolls the texts goes to where its text is, the row it's in, and once that's drawn and it isn't, as the
//! text of a closed thought isn't, there's nothing more to do.
//!
//! A match is brought in with room above it, since it's gone to from wherever the chat was. The end of what's selected
//! is brought in by as little as it takes, as a drag held past the edge scrolls only as far as the pointer goes, since
//! a key moves it a character or a line at a time, so a key held down scrolls a line at a time.
//!
//! Markdown, which the transcript draws from the kit's blocks, says where a match is as plain text does, and the
//! selection says where the end is from where its text was laid out.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::{Bounds, ElementId, IntoElement, Pixels, Styled as _, TextLayout, canvas, px};

use crate::texts::Of;

/// How much room is left above the match when something scrolls to it, so it isn't on the very edge.
const ROOM: Pixels = px(48.);

/// How many frames to go on trying to bring something into view at most, which takes a few once it's drawn, and more
/// only while what it's drawn in won't settle, such as a reply that's coming in.
const FRAMES: u32 = 120;

/// What's to be brought into view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wanted {
    /// The match Find is showing.
    Match,
    /// The end of what's selected that a key moved.
    End,
}

/// What's to be brought into view in one view, and where it was last drawn. Cheap to clone.
#[derive(Clone, Default)]
pub(crate) struct Reveal(Rc<RefCell<Slot>>);

#[derive(Default)]
struct Slot {
    /// What's to be brought into view, if anything is.
    wanted: Option<Wanted>,
    /// Where its text is, for what scrolls the texts to go to as the next frame starts, while it hasn't been drawn
    /// since it was wanted.
    at: Option<Of>,
    /// Where it was drawn in the last frame, unless what it's drawn in scrolled after.
    drawn: Option<Drawn>,
    /// Whether an area that scrolls what it's in has scrolled since it was last drawn, so it's still to be drawn there.
    moved: bool,
    /// How many more frames to go on trying.
    frames: u32,
    /// What of the view was in view as the last frame started, for a test to see whether the match ended up in it.
    #[cfg(any(test, feature = "testing"))]
    visible: Option<Bounds<Pixels>>,
    /// Where the match was last drawn, wanted or not, for a test to see where it ended up.
    #[cfg(any(test, feature = "testing"))]
    painted: Option<Bounds<Pixels>>,
}

#[derive(Clone, Debug)]
pub(crate) struct Drawn {
    /// The area it's in that scrolls, if it's in one.
    pub(crate) area: Option<ElementId>,
    /// Where it was drawn, in the window.
    pub(crate) bounds: Bounds<Pixels>,
}

/// What what scrolls the texts is to do for what's wanted, as a frame starts.
#[derive(Debug, PartialEq)]
pub(crate) enum Scroll {
    /// Go to where this is, which what's wanted is in, since it hasn't been drawn since it was wanted.
    Show(Of),
    /// Scroll down by this, or up for less than nothing, toward it.
    By(Pixels),
}

impl std::fmt::Debug for Reveal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let slot = self.0.borrow();
        f.debug_struct("Reveal").field("wanted", &slot.wanted).field("drawn", &slot.drawn).finish_non_exhaustive()
    }
}

impl Reveal {
    /// Asks for `what`, in a text that's `at` there, to be brought into view, instead of anything asked for before,
    /// forgetting where that was drawn.
    pub(crate) fn want(&self, what: Wanted, at: Of) {
        let mut slot = self.0.borrow_mut();
        slot.wanted = Some(what);
        (slot.at, slot.drawn, slot.moved) = (Some(at), None, false);
        slot.frames = FRAMES;
        #[cfg(any(test, feature = "testing"))]
        if what == Wanted::Match {
            slot.painted = None;
        }
    }

    /// Whether `what` is to be brought into view.
    pub(crate) fn wants(&self, what: Wanted) -> bool {
        self.0.borrow().wanted == Some(what)
    }

    /// Whether anything is, which the next frame goes on with.
    pub(crate) fn wants_anything(&self) -> bool {
        self.0.borrow().wanted.is_some()
    }

    /// It's in view, or nothing more can be done.
    pub(crate) fn done(&self) {
        let mut slot = self.0.borrow_mut();
        slot.wanted = None;
        slot.drawn = None;
    }

    /// `what` isn't to be brought into view any more, if it was, such as the match once Find's bar closes.
    pub(crate) fn done_with(&self, what: Wanted) {
        if self.wants(what) {
            self.done();
        }
    }

    /// Says `what` was drawn at `bounds`, in `area` if it's in an area that scrolls, which counts while it's wanted.
    pub(crate) fn drawn_at(&self, what: Wanted, bounds: Bounds<Pixels>, area: Option<ElementId>) {
        let mut slot = self.0.borrow_mut();
        #[cfg(any(test, feature = "testing"))]
        if what == Wanted::Match {
            slot.painted = Some(bounds);
        }
        if slot.wanted == Some(what) {
            (slot.at, slot.drawn, slot.moved) = (None, Some(Drawn { area, bounds }), false);
        }
    }

    /// As a frame of what scrolls the texts starts, with what's in `visible` in view: what it's to do toward what's
    /// wanted, from where that was drawn in the last frame, which is then out of date, or nothing, while it waits a
    /// frame to be drawn where an area scrolled it, or once nothing's wanted. It's done once that's in view, or once it
    /// isn't drawn though its text has been gone to, and gives up once it has no frames left.
    pub(crate) fn as_frame_starts(&self, visible: Bounds<Pixels>) -> Option<Scroll> {
        let mut slot = self.0.borrow_mut();
        #[cfg(any(test, feature = "testing"))]
        {
            slot.visible = Some(visible);
        }
        let wanted = slot.wanted?;
        let Some(frames) = slot.frames.checked_sub(1) else {
            slot.wanted = None;
            return None;
        };
        slot.frames = frames;
        let Some(drawn) = slot.drawn.take() else {
            if std::mem::take(&mut slot.moved) {
                return None;
            }
            let at = slot.at.take();
            slot.wanted = at.is_some().then_some(wanted);
            return at.map(Scroll::Show);
        };
        let by = scrolls(wanted, visible, drawn.bounds).0;
        slot.wanted = by.is_some().then_some(wanted);
        by.map(Scroll::By)
    }

    /// How far the area `area`, which shows what's in `visible`, is to scroll down and right to bring what's wanted
    /// into its view, if that was drawn in it, out of its view. Once the area has, it says so with [`Reveal::moved`].
    pub(crate) fn in_area(
        &self,
        area: &ElementId,
        visible: Bounds<Pixels>,
    ) -> Option<(Option<Pixels>, Option<Pixels>)> {
        let slot = self.0.borrow();
        let wanted = slot.wanted?;
        let drawn = slot.drawn.as_ref().filter(|drawn| drawn.area.as_ref() == Some(area))?;
        let (down, across) = scrolls(wanted, visible, drawn.bounds);
        (down.is_some() || across.is_some()).then_some((down, across))
    }

    /// What it's drawn in has scrolled, so where it was drawn is out of date until it's drawn again.
    pub(crate) fn moved(&self) {
        let mut slot = self.0.borrow_mut();
        (slot.drawn, slot.moved) = (None, true);
    }

    /// Whether the match was last drawn in view below the bar, once it has been drawn since it was wanted.
    #[cfg(any(test, feature = "testing"))]
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
            let bounds = Bounds::new(at, gpui_kit::size(px(1.), layout.line_height()));
            reveal.drawn_at(Wanted::Match, bounds, area.clone());
        },
    )
    .absolute()
    .size_0()
}

/// How far something showing `visible` would scroll down, and right, to bring `wanted`, drawn at `bounds`, into view,
/// each if it isn't in view that way: a match with [`ROOM`] before it, and the end by as little as it takes. Up and
/// left are less than nothing.
fn scrolls(wanted: Wanted, visible: Bounds<Pixels>, bounds: Bounds<Pixels>) -> (Option<Pixels>, Option<Pixels>) {
    let by: fn(Range<Pixels>, Range<Pixels>) -> Option<Pixels> = match wanted {
        Wanted::Match => with_room,
        Wanted::End => least_scroll,
    };
    let down = by(visible.top()..visible.bottom(), bounds.top()..bounds.bottom());
    (down, by(visible.left()..visible.right(), bounds.left()..bounds.right()))
}

/// How far something showing `visible` would scroll on to bring `bounds` into view with [`ROOM`] before it, but at
/// most a quarter of the view, along one way, down or across, if it isn't in view that way: back is less than nothing.
fn with_room(visible: Range<Pixels>, bounds: Range<Pixels>) -> Option<Pixels> {
    let inside = bounds.start >= visible.start && bounds.end <= visible.end;
    (!inside).then(|| bounds.start - visible.start - ROOM.min((visible.end - visible.start) / 4.))
}

/// How far something showing `visible` would scroll on to bring `bounds` just into view, along one way, down or across,
/// if it isn't in view that way: back is less than nothing. What's as long as the view, or longer, is brought in from
/// its start.
fn least_scroll(visible: Range<Pixels>, bounds: Range<Pixels>) -> Option<Pixels> {
    let by = if bounds.start < visible.start || bounds.end - bounds.start >= visible.end - visible.start {
        bounds.start - visible.start
    } else {
        (bounds.end - visible.end).max(Pixels::ZERO)
    };
    (by != Pixels::ZERO).then_some(by)
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, ElementId, Pixels, point, px, size};
    use protocol::model::Seq;

    use super::{Reveal, Scroll, Wanted, scrolls};
    use crate::texts::Of;

    /// What a view `tall` and 300 wide, from the window's top left, shows.
    fn view(tall: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(0.), px(0.)), size(px(300.), px(tall)))
    }

    /// Something a line, 20, tall, and 1 wide, drawn from `x`, `y`.
    fn at(x: f32, y: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(1.), px(20.)))
    }

    #[test]
    fn a_match_comes_into_view_with_room_before_it_and_the_end_by_as_little_as_it_takes() {
        let match_ = |visible, drawn| scrolls(Wanted::Match, visible, drawn);
        let end = |visible, drawn| scrolls(Wanted::End, visible, drawn);

        assert_eq!(match_(view(400.), at(10., 500.)), (Some(px(452.)), None), "below, with 48 above it");
        assert_eq!(end(view(400.), at(10., 500.)), (Some(px(120.)), None), "below, to the view's bottom");
        assert_eq!(match_(view(400.), at(10., -100.)), (Some(px(-148.)), None), "above");
        assert_eq!(end(view(400.), at(10., -100.)), (Some(px(-100.)), None), "above, to the view's top");
        assert_eq!(match_(view(400.), at(400., 100.)), (None, Some(px(352.))), "off to the right, across");
        assert_eq!(end(view(400.), at(400., 100.)), (None, Some(px(101.))), "off to the right, to the view's edge");
        assert_eq!(match_(view(100.), at(10., 200.)), (Some(px(175.)), None), "with room a quarter of a short view");
        assert_eq!(end(view(10.), at(10., 50.)), (Some(px(50.)), None), "taller than the view, from its top");
        assert_eq!(match_(view(400.), at(10., 100.)), (None, None), "in view");
        assert_eq!(end(view(400.), at(10., 100.)), (None, None), "in view");
    }

    #[test]
    fn it_goes_to_its_text_until_it_s_drawn_and_waits_for_it_only_where_an_area_scrolled_it() {
        let (reveal, of, area) = (Reveal::default(), Of::Event(Seq::new(1)), ElementId::Name("output".into()));

        reveal.want(Wanted::Match, of);
        assert_eq!(reveal.as_frame_starts(view(400.)), Some(Scroll::Show(of)), "to its row first");
        assert_eq!(reveal.as_frame_starts(view(400.)), None, "and not drawn after that, as a closed thought isn't");
        assert!(!reveal.wants(Wanted::Match), "it's done with it");

        reveal.want(Wanted::End, of);
        reveal.drawn_at(Wanted::End, at(10., 500.), Some(area));
        reveal.moved();
        assert_eq!(reveal.as_frame_starts(view(400.)), None, "where an area scrolled it, it waits to be drawn there");
        assert!(reveal.wants(Wanted::End), "rather than being done");
        reveal.drawn_at(Wanted::Match, at(10., 500.), None);
        assert_eq!(reveal.as_frame_starts(view(400.)), None, "a match drawn isn't the end");
        assert!(!reveal.wants(Wanted::End), "which, drawn nowhere, it's done with");

        reveal.want(Wanted::End, of);
        reveal.drawn_at(Wanted::End, at(10., 500.), None);
        assert_eq!(reveal.as_frame_starts(view(400.)), Some(Scroll::By(px(120.))), "drawn, it's scrolled to");
        reveal.drawn_at(Wanted::End, at(10., 380.), None);
        assert_eq!(reveal.as_frame_starts(view(400.)), None, "until it's in view");
        assert!(!reveal.wants(Wanted::End), "when it's done");
    }
}
