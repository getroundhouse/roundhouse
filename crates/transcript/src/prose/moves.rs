//! Where each text is in the chat now. A text is kept by what it's known by, which stays the same as it moves: as a
//! reply does from being written to its record, or a prompt from waiting at the end of the chat to the row where the
//! agent took it. So a text drawn somewhere new is drawn at once from what it was parsed as, a parse on another thread
//! lands wherever the text is when it's in, and a selection that names it, as `crate::selection` does, stays in it.
//! Where it is orders it among the others.
//!
//! A text drawn where another was takes its place, as a prompt waiting at the end of the chat does when the one before
//! it is taken, since the list draws the end before the row where the agent took that one. The other has no place
//! until it's drawn again, but keeps where it was last drawn, which says whether it's still loaded, so it's kept, with
//! what it was parsed as, as long as it would have been there.

use std::collections::{BTreeMap, HashMap};
use std::ops::RangeBounds;

use super::{Prose, ProseId, Store, TextAt};

/// Where each text was last drawn, and in which frame, and which text is at each place now, in the chat's order. Each
/// place has one text, which was last drawn there, and a text drawn in one place a frame, as a debug build checks.
#[derive(Default)]
pub(super) struct Places {
    last: HashMap<ProseId, Drawn>,
    now: BTreeMap<TextAt, ProseId>,
}

/// Where a text was last drawn, and in which frame.
#[derive(Clone, Copy)]
struct Drawn {
    at: TextAt,
    frame: u64,
}

impl Places {
    /// Notes that the text known as `id` is drawn at `at` in the `frame`th frame. A text that was here isn't now.
    pub(super) fn place(&mut self, id: &ProseId, at: TextAt, frame: u64) {
        if let Some(was) = self.last.insert(id.clone(), Drawn { at, frame }) {
            debug_assert!(
                was.at == at || was.frame < frame,
                "{id:?} is drawn at {:?} and at {at:?} in one frame, so the two would take turns being it",
                was.at
            );
            if was.at != at && self.now.get(&was.at) == Some(id) {
                self.now.remove(&was.at);
            }
        }
        if let Some(other) = self.now.insert(at, id.clone()).filter(|other| other != id) {
            debug_assert!(
                self.last.get(&other).is_none_or(|other| other.frame < frame),
                "{other:?} and {id:?} are both drawn at {at:?} in one frame, so they'd take turns being there"
            );
        }
    }

    /// Where the text known as `id` is now, if it has a place.
    pub(super) fn of(&self, id: &ProseId) -> Option<TextAt> {
        let at = self.last.get(id)?.at;
        (self.now.get(&at) == Some(id)).then_some(at)
    }

    /// What the text at `at` is known by, if one's there.
    #[cfg(test)]
    pub(super) fn at(&self, at: &TextAt) -> Option<&ProseId> {
        self.now.get(at)
    }

    /// The texts with a place in `range`, in the chat's order.
    pub(super) fn within(
        &self,
        range: impl RangeBounds<TextAt>,
    ) -> impl DoubleEndedIterator<Item = (&TextAt, &ProseId)> {
        self.now.range(range)
    }

    /// Where the text known as `id` was last drawn, if it's kept, whether or not another has taken its place since.
    pub(super) fn last_at(&self, id: &ProseId) -> Option<TextAt> {
        self.last.get(id).map(|drawn| drawn.at)
    }

    /// The frame the text known as `id` was last drawn in, if it's kept.
    pub(super) fn last_drawn(&self, id: &ProseId) -> Option<u64> {
        self.last.get(id).map(|drawn| drawn.frame)
    }

    /// Lets go of each text last drawn where `keep` says not to keep.
    pub(super) fn retain(&mut self, keep: impl Fn(&TextAt) -> bool) {
        self.last.retain(|_, drawn| keep(&drawn.at));
        // The text at each place was last drawn there, so it goes with the place.
        self.now.retain(|at, _| keep(at));
    }
}

impl Store {
    /// Notes that the text known as `id` is drawn at `at` in this frame.
    pub(super) fn placed(&mut self, id: &ProseId, at: TextAt) {
        let frame = self.frame;
        self.places.place(id, at, frame);
    }
}

impl Prose {
    /// Where the text known as `id` is now, if it's kept and has a place.
    pub(crate) fn place_of(&self, id: &ProseId) -> Option<TextAt> {
        self.0.borrow().places.of(id)
    }

    /// Where the text known as `id` was last drawn, if it's kept, whether or not another has taken its place since,
    /// which is where it's kept for.
    pub(crate) fn last_drawn_at(&self, id: &ProseId) -> Option<TextAt> {
        self.0.borrow().places.last_at(id)
    }
}

#[cfg(test)]
impl ProseId {
    /// One known as `name`, for a test.
    pub(crate) fn named(name: impl Into<gpui_kit::SharedString>) -> Self {
        Self(gpui_kit::ElementId::Name(name.into()))
    }

    /// One for a test, known by the place it's first drawn at.
    pub(crate) fn of(at: TextAt) -> Self {
        Self::named(format!("{at:?}"))
    }
}

#[cfg(test)]
impl Prose {
    /// As if the text known as `id`, saying `text`, were drawn at `at`.
    pub(crate) fn drawn_as(&self, id: &ProseId, at: TextAt, text: &str) {
        self.0.borrow_mut().placed(id, at);
        self.parsed(id, text);
    }

    /// As if the text known by `at`, saying `text`, were drawn there.
    pub(crate) fn drawn_at(&self, at: TextAt, text: &str) {
        self.drawn_as(&ProseId::of(at), at, text);
    }

    /// What the text at `at` is known by, if one's there.
    pub(crate) fn known_at(&self, at: TextAt) -> Option<ProseId> {
        self.0.borrow().places.at(&at).cloned()
    }

    /// Whether the text known as `id` is kept, with what it was parsed as or what it shows.
    pub(crate) fn keeps(&self, id: &ProseId) -> bool {
        self.0.borrow().texts.contains_key(id)
    }
}

#[cfg(test)]
mod tests;
