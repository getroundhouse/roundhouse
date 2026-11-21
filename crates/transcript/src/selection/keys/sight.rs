//! Keeping the end of what's selected in view as keys move it, as a drag keeps its end in view: through the frame of
//! whatever scrolls the texts, the chat's list, a document's or a page, which each frame scrolls for a key as for a
//! drag, as `frames` says.
//!
//! As a drag goes by what shows, so does a key, but for the lines of the end's own text, which an area that scrolls it
//! scrolls to. Where nothing shows that a move can go to, such as past the window's bottom, the move waits while what
//! scrolls the texts scrolls a line that way, a frame at a time, as it does under a drag held a line past the edge,
//! until something does, or a frame's scroll moves nothing, as at the chat's end. A row can hold no text, such as a
//! call's line, so that can take a few lines. Once the end has moved, the view's `Reveal` brings it into view by as
//! little as it takes, as it brings Find's match into view, first by the area that scrolls its text, if it's in one,
//! and then by what scrolls the texts, as `crate::reveal` says.
//!
//! The one thing a key meets that a drag doesn't is an end out of view, since a drag's end is under the pointer: the
//! chat can be scrolled away from what's selected, and a key pressed after. Then the `Reveal` brings the end into view
//! first, as a text box goes back to its caret, going to the row its text is in, and the move goes on once it's drawn.

#[cfg(any(test, feature = "testing"))]
use gpui_kit::Bounds;
use gpui_kit::Pixels;

use super::{Step, Tried, Way};
use crate::reveal::Wanted;
use crate::selection::{End, Scroller, Selection};

/// A move the keys have still to make, of the end they moved last, which waits for what it goes to, or for the end, to
/// be drawn.
#[derive(Debug)]
pub(super) struct ToDo {
    /// The end it's for, which it's let go of once anything else moves.
    end: End,
    /// The move.
    step: Step,
    /// Whether what scrolls the texts has scrolled past what showed when the move was made, for it.
    past: bool,
    /// How far to scroll in a frame while the move waits: a line of the end's text, as last drawn.
    line: Option<Pixels>,
    /// How far what scrolls the texts is to scroll down as the next frame starts, or up for less than nothing, while
    /// the move waits for what it goes to to show.
    scroll: Option<Pixels>,
    /// Whether the view's `Reveal` has been asked to bring the end into view, since it isn't drawn, or to go to where
    /// the move goes, which the move waits for.
    drawing: bool,
}

impl ToDo {
    /// Moving `end` by `step`.
    pub(super) fn moving(end: End, step: Step) -> Self {
        Self { end, step, past: false, line: None, scroll: None, drawing: false }
    }

    /// Whether it's a move by `step` from `end`, which a key held down, repeating, goes on with.
    pub(super) fn waits_to(&self, end: &End, step: Step) -> bool {
        self.end == *end && self.step == step
    }
}

impl Selection {
    /// Goes on with the move the keys left, by the texts drawn last: makes it, if what it waits for is drawn, and has
    /// the end brought into view, or keeps it for the next frame, with a line to scroll as that starts, or the end to
    /// be drawn.
    pub(super) fn go_on_by_keys(&self) {
        self.let_go_of_an_end_left_behind();
        let Some(mut to_do) = self.inner.borrow_mut().keyed.to_do.take() else { return };
        to_do.scroll = None;
        match self.try_move(&to_do.end, to_do.step, to_do.past) {
            Tried::Moved(end, run) => self.moved_to(end, run),
            Tried::Past(way) => {
                let line = self.end_drawn(&self.inner.borrow(), &to_do.end).map(|(caret, _)| caret.size.height);
                to_do.line = line.or(to_do.line);
                let Some(by) = to_do.line else { return };
                to_do.past = true;
                to_do.scroll = Some(match way {
                    Way::Back => -by,
                    Way::On => by,
                });
                self.inner.borrow_mut().keyed.to_do = Some(to_do);
            }
            // Until the `Reveal` has the end drawn, or what it goes to, or gives up, as for a text in a row that shows
            // without it.
            Tried::NotDrawn | Tried::Elsewhere(_) if to_do.drawing => {
                if self.reveal.wants(Wanted::End) {
                    self.inner.borrow_mut().keyed.to_do = Some(to_do);
                }
            }
            Tried::NotDrawn => {
                if self.bring_into_view(to_do.end.clone()) {
                    self.inner.borrow_mut().keyed.to_do = Some(ToDo { drawing: true, ..to_do });
                }
            }
            // To where what it goes to is, which draws it, as the `Reveal` goes to an end's row. The end it moves from
            // isn't what's brought into view any more, so a frame that draws it doesn't take the `Reveal` back to it.
            Tried::Elsewhere(of) => {
                self.reveal.want(Wanted::End, of);
                let mut selecting = self.inner.borrow_mut();
                selecting.keyed.revealing = None;
                selecting.keyed.to_do = Some(ToDo { drawing: true, ..to_do });
            }
            Tried::Nowhere => {}
        }
    }

    /// As a frame starts, before the texts are laid out, and with no drag to scroll for: scrolls `scroller` the line
    /// the keys left for it, if they left one, saying whether they did.
    pub(in crate::selection) fn scroll_for_keys(&self, scroller: &dyn Scroller) -> bool {
        self.let_go_of_an_end_left_behind();
        let by = self.inner.borrow_mut().keyed.to_do.as_mut().and_then(|to_do| to_do.scroll.take());
        by.inspect(|by| scroller.scroll_down(*by)).is_some()
    }

    /// A frame's scroll for the keys moved nothing, as at the chat's start or end, so what they waited for isn't there.
    pub(in crate::selection) fn keys_went_nowhere(&self) {
        self.inner.borrow_mut().keyed.to_do = None;
    }

    /// Once the texts have said where they were drawn: goes on with the move the keys left, and says where the end
    /// being brought into view was drawn, for the view's `Reveal`. Says whether they've a line to scroll as the next
    /// frame starts.
    pub(in crate::selection) fn keys_laid_out(&self) -> bool {
        if self.inner.borrow().keyed.to_do.is_some() {
            self.go_on_by_keys();
        }
        let selecting = self.inner.borrow();
        if let Some(end) = selecting.keyed.revealing.as_ref().filter(|_| self.reveal.wants(Wanted::End))
            && let Some((caret, drawn)) = self.end_drawn(&selecting, end)
        {
            self.reveal.drawn_at(Wanted::End, caret, drawn.area.clone());
        }
        selecting.keyed.to_do.as_ref().is_some_and(|to_do| to_do.scroll.is_some())
    }

    /// Has the view's `Reveal` bring `end` into view, from where it was drawn in the last frame, if it was, so the next
    /// scrolls to it at once, even with a key held down repeating faster than that, and otherwise once it's drawn,
    /// going to the row its text was last drawn in first. Says whether it could, which it can't for a text never drawn.
    pub(super) fn bring_into_view(&self, end: End) -> bool {
        let Some(at) = self.texts.last_drawn_at(&end.text) else { return false };
        // In an area that scrolls, which goes first, it's for the frame to say where it's drawn, as the area paints.
        let drawn = self
            .end_drawn(&self.inner.borrow(), &end)
            .filter(|(_, drawn)| drawn.area.is_none())
            .map(|(caret, _)| caret);
        self.inner.borrow_mut().keyed.revealing = Some(end);
        self.reveal.want(Wanted::End, at.of);
        if let Some(caret) = drawn {
            self.reveal.drawn_at(Wanted::End, caret, None);
        }
        true
    }

    /// Whether the end of what's selected that moved last was drawn in the last frame within `bounds`, and within what
    /// shows of its text, if it was drawn, for a test of what's kept in view.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn end_in(&self, bounds: Bounds<Pixels>) -> Option<bool> {
        let within = |caret: Bounds<Pixels>, bounds: Bounds<Pixels>| {
            caret.top() >= bounds.top() && caret.bottom() <= bounds.bottom()
        };
        let selecting = self.inner.borrow();
        let (caret, drawn) = self.end_drawn(&selecting, &selecting.ends.as_ref()?.1)?;
        Some(within(caret, bounds) && within(caret, drawn.bounds))
    }

    /// Lets go of what the keys have left to do, as for a view that wasn't drawn in the last frame, such as another
    /// tab's, which comes back to them where it was.
    pub(in crate::selection) fn let_go_of_the_keys(&self) {
        let mut selecting = self.inner.borrow_mut();
        let to_do = selecting.keyed.to_do.take();
        if selecting.keyed.revealing.take().is_some() || to_do.is_some_and(|to_do| to_do.drawing) {
            self.reveal.done_with(Wanted::End);
        }
    }

    /// Once anything else has moved the end that was being brought into view, or that a move was waiting for, such as
    /// a press, it's let go of.
    fn let_go_of_an_end_left_behind(&self) {
        let mut selecting = self.inner.borrow_mut();
        let far = selecting.ends.as_ref().map(|(_, far)| far.clone());
        let keyed = &mut selecting.keyed;
        if keyed.revealing.as_ref().is_some_and(|end| far.as_ref() != Some(end)) {
            keyed.revealing = None;
            self.reveal.done_with(Wanted::End);
        }
        if let Some(to_do) = keyed.to_do.take_if(|to_do| far.as_ref() != Some(&to_do.end))
            && to_do.drawing
        {
            self.reveal.done_with(Wanted::End);
        }
    }
}
