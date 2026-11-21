//! The element that holds whatever scrolls a selection's texts, such as the chat's list, a document's list of blocks
//! or a page, and does for it in each frame what the selection needs, the same for each. Before what it holds lays
//! the texts out, it ends a drag it wasn't drawn for, and scrolls: on under a pointer held past its top or bottom, as
//! the keys that scroll it as a page asked, as `paging` says, for the keys that move an end, as `keys` says, a line on
//! while a move waits for what shows, or toward what the view's `Reveal` is bringing into view, such as the end a key
//! moved or Find's match. After, once the texts have said where they were drawn, a scroll for the keys that move an end
//! or the `Reveal` that moved nothing ends what it was for, since it can't go further that way, a drag's far end moves
//! to what's under the pointer, and a move by a key that waited goes on. It asks for the next frame while any of them
//! has more to do. It hears the pointer under the texts, for a press to start a
//! selection.
//! Only how what it holds scrolls is each one's own, as [`Scroller`] says.

use std::time::Instant;

use gpui_kit::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId,
    IntoElement, LayoutId, Pixels, ScrollHandle, Style, Window, point, px, relative,
};

use super::Selection;
use crate::reveal::Scroll;
use crate::texts::Of;

/// What scrolls a selection's texts, as [`Scrolled`] has it scroll for the selection.
pub(crate) trait Scroller: 'static {
    /// Scrolls what it shows down by `by`, or up for less than nothing, as far as it goes, once it's laid out.
    fn scroll_down(&self, by: Pixels);

    /// How far down what it shows starts. Read once it's laid out, that's after a scroll is clamped to how far it goes,
    /// so it says whether a scroll as the frame started moved it.
    fn scrolled(&self) -> Pixels;

    /// What of `bounds`, where it shows the texts, is in view, for what the view's `Reveal` brings into view, which is
    /// less where something lies over it, such as Find's bar over the chat.
    fn in_view(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        bounds
    }

    /// Goes to where what `of` says is drawn, if it didn't show in the last frame, saying whether it went. What draws
    /// all it holds, as a page does, has nothing to go to.
    fn show(&self, of: Of) -> bool {
        let _ = of;
        false
    }

    /// Goes to the start of what it shows, for the key that goes there, as far up as it has.
    fn to_start(&self);

    /// Goes on to the end of what it shows, for the key that goes there, setting off `now` if it goes there over a
    /// while.
    fn to_end(&self, now: Instant);
}

/// What scrolls by GPUI's own handle, such as a page drawn whole.
impl Scroller for ScrollHandle {
    fn scroll_down(&self, by: Pixels) {
        let (offset, furthest) = (self.offset(), self.max_offset().y);
        self.set_offset(point(offset.x, (offset.y - by).clamp(-furthest, px(0.))));
    }

    fn scrolled(&self) -> Pixels {
        -self.offset().y
    }

    fn to_start(&self) {
        self.set_offset(point(self.offset().x, px(0.)));
    }

    fn to_end(&self, _: Instant) {
        self.set_offset(point(self.offset().x, -self.max_offset().y));
    }
}

/// `held`, which draws a selection's texts and scrolls by `scroller`, with what the selection needs done each frame.
pub(crate) struct Scrolled<S> {
    id: ElementId,
    scroller: S,
    selection: Selection,
    held: AnyElement,
}

impl<S: Scroller> Scrolled<S> {
    /// `id` keeps what the frames before knew of it, such as whether it was drawn in the last, so it's its own.
    pub(crate) fn new(id: impl Into<ElementId>, scroller: S, selection: &Selection, held: impl IntoElement) -> Self {
        Self { id: id.into(), scroller, selection: selection.clone(), held: held.into_any_element() }
    }
}

impl<S: Scroller> IntoElement for Scrolled<S> {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl<S: Scroller> Element for Scrolled<S> {
    type RequestLayoutState = ();
    /// Where the selection hears the pointer.
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let held = self.held.request_layout(window, cx);
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        style.min_size.height = px(0.).into();
        (window.request_layout(style, [held], cx), ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Hitbox {
        // A drag ends as the button comes up, which is only heard while this is drawn, and what the keys were doing
        // was for a view that's gone from the window since.
        if !drawn_last_frame(id, window) {
            self.selection.release();
            self.selection.let_go_of_the_keys();
        }
        let frame = self.selection.frame_started(bounds, &self.scroller, cx.background_executor().now());
        // Under the texts, so what's in them, such as a link, takes a press before the selection does.
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        self.held.prepaint(window, cx);
        if self.selection.texts_laid_out(bounds, &self.scroller, frame) {
            window.request_animation_frame();
        }
        hitbox
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        hitbox: &mut Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.selection.hear_the_pointer(hitbox, bounds, window);
        self.held.paint(window, cx);
    }
}

/// A frame of what scrolls the texts, as it started: what it scrolled for, which a scroll that moved nothing ends, from
/// how far it was scrolled, and whether it scrolled for a drag, which goes on in the next. What the keys that scroll it
/// as a page asked is done once, whatever it moves.
#[derive(Clone, Copy, Debug)]
pub(in crate::selection) struct Frame {
    scrolled_for: Option<ScrolledFor>,
    from: Pixels,
    dragged: bool,
}

/// What a frame scrolled for as it started, which goes on only if that moved anything. A drag held past the edge
/// scrolls on for as long as it's held, whatever that moves.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ScrolledFor {
    /// What the keys left to do: a line on while a move waits for what shows.
    Keys,
    /// What the view's `Reveal` is bringing into view.
    Reveal,
}

impl Selection {
    /// Starts a frame of what scrolls the texts, `scroller`, in `bounds`, at `now`, before it lays them out: forgets
    /// the texts drawn in the last, before this one's are, and scrolls it for a drag held past its top or bottom, if
    /// one is, or else as the keys that scroll it as a page asked, or else for the keys that move an end, or else
    /// toward what the view's `Reveal` is bringing into view, or to its text's row.
    pub(in crate::selection) fn frame_started(
        &self,
        bounds: Bounds<Pixels>,
        scroller: &dyn Scroller,
        now: Instant,
    ) -> Frame {
        self.new_frame();
        let from = scroller.scrolled();
        if let Some(by) = self.scroll_past_edge(bounds, now) {
            // The drag takes over from what was being brought into view, which would scroll back to it after.
            self.reveal.done();
            scroller.scroll_down(by);
            return Frame { scrolled_for: None, from, dragged: true };
        }
        if self.paging.scroll(scroller, scroller.in_view(bounds), now) {
            // What was being brought into view gives way, as it does to a drag, rather than scroll back to it after.
            self.reveal.done();
            return Frame { scrolled_for: None, from, dragged: false };
        }
        let scrolled_for = if self.scroll_for_keys(scroller) {
            Some(ScrolledFor::Keys)
        } else {
            let went = match self.reveal.as_frame_starts(scroller.in_view(bounds)) {
                None => false,
                Some(Scroll::Show(of)) => scroller.show(of),
                Some(Scroll::By(by)) => {
                    scroller.scroll_down(by);
                    true
                }
            };
            went.then_some(ScrolledFor::Reveal)
        };
        Frame { scrolled_for, from, dragged: false }
    }

    /// After the texts in `bounds`, which `scroller` scrolls, are laid out in the `frame` that started, and have said
    /// where they were drawn: a scroll for the keys or the `Reveal` that moved nothing ends what it was for, a drag's
    /// far end follows what's under the pointer, so it keeps up as they move under a pointer held still, and the keys
    /// go on with what they left to do. Says whether to ask for the next frame, which goes on with whatever's left: a
    /// drag held past the edge, a line the keys are to scroll, or what the `Reveal` is bringing into view.
    pub(in crate::selection) fn texts_laid_out(
        &self,
        bounds: Bounds<Pixels>,
        scroller: &dyn Scroller,
        frame: Frame,
    ) -> bool {
        let moved = scroller.scrolled() != frame.from;
        match frame.scrolled_for {
            Some(ScrolledFor::Keys) if !moved => self.keys_went_nowhere(),
            Some(ScrolledFor::Reveal) if !moved => self.reveal.done(),
            Some(ScrolledFor::Keys | ScrolledFor::Reveal) | None => {}
        }
        let followed = self.follow(bounds);
        let keyed = self.keys_laid_out();
        frame.dragged || followed || keyed || self.reveal.wants_anything()
    }
}

/// Whether the element known as `id` was drawn in the last frame, as it wasn't when another tab or workspace was shown,
/// noting that it's drawn in this one. What hears a button come up only while it's drawn takes one it wasn't drawn
/// for as having come up.
pub(crate) fn drawn_last_frame(id: Option<&GlobalElementId>, window: &mut Window) -> bool {
    id.is_some_and(|id| window.with_element_state(id, |was: Option<()>, _| (was, ())).is_some())
}
