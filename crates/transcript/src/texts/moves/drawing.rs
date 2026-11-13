use super::super::{Of, Store, TextAt, TextKey, Texts};
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::RangeBounds;

/// How many texts the end of the chat drew, and doesn't draw now, it keeps for their records.
const LEFT_AT_THE_END: usize = 8;

/// Where each text was last drawn, and in which frame, and which text is at each place now, in the chat's order. Each
/// place has one text, which was last drawn there, and a text drawn in one place a frame, as a debug build checks.
#[derive(Default)]
pub(in super::super) struct Places {
    pub(super) last: HashMap<TextKey, Drawn>,
    pub(super) now: BTreeMap<TextAt, TextKey>,
}

/// Where a text was last drawn, and in which frame.
#[derive(Clone, Copy)]
pub(super) struct Drawn {
    pub(super) at: TextAt,
    pub(super) frame: u64,
}

impl Places {
    /// Notes that the text known as `id` is drawn at `at` in the `frame`th frame. A text that was here isn't now.
    pub(in super::super) fn place(&mut self, id: &TextKey, at: TextAt, frame: u64) {
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
    pub(in super::super) fn of(&self, id: &TextKey) -> Option<TextAt> {
        let at = self.last.get(id)?.at;
        (self.now.get(&at) == Some(id)).then_some(at)
    }

    /// What the text at `at` is known by, if one's there.
    #[cfg(test)]
    pub(in super::super) fn at(&self, at: &TextAt) -> Option<&TextKey> {
        self.now.get(at)
    }

    /// The texts with a place in `range`, in the chat's order.
    pub(in super::super) fn within(
        &self,
        range: impl RangeBounds<TextAt>,
    ) -> impl DoubleEndedIterator<Item = (&TextAt, &TextKey)> {
        self.now.range(range)
    }

    /// Where the text known as `id` was last drawn, if it's kept, whether or not another has taken its place since.
    pub(in super::super) fn last_at(&self, id: &TextKey) -> Option<TextAt> {
        self.last.get(id).map(|drawn| drawn.at)
    }

    /// The frame the text known as `id` was last drawn in, if it's kept.
    pub(in super::super) fn last_drawn(&self, id: &TextKey) -> Option<u64> {
        self.last.get(id).map(|drawn| drawn.frame)
    }

    /// The text at `at` isn't drawn there now, and has no place until it's drawn again. It's kept, by where it was last
    /// drawn.
    pub(in super::super) fn leave(&mut self, at: &TextAt) {
        self.now.remove(at);
    }

    /// Lets go of each text last drawn where `keep` says not to keep.
    pub(in super::super) fn retain(&mut self, keep: impl Fn(&TextAt) -> bool) {
        self.last.retain(|_, drawn| keep(&drawn.at));
        // The text at each place was last drawn there, so it goes with the place.
        self.now.retain(|at, _| keep(at));
    }

    /// Lets go of the texts the end of the chat drew longest ago, and doesn't draw now, since it's moved on past them
    /// or another has taken their place, but for the [`LEFT_AT_THE_END`] it drew most recently, and those `held`.
    pub(in super::super) fn keep_few_left_at_the_end(
        &mut self,
        hidden: &HashSet<TextKey>,
        held: impl Fn(&TextAt) -> bool,
    ) {
        let left_there = |(id, drawn): &(&TextKey, &Drawn)| {
            drawn.at.of == Of::Tail && !held(&drawn.at) && (hidden.contains(*id) || self.of(id).is_none())
        };
        let mut left: Vec<(u64, TextAt, TextKey)> =
            self.last.iter().filter(left_there).map(|(id, drawn)| (drawn.frame, drawn.at, id.clone())).collect();
        // Those left in one frame by where they were, so which go doesn't hang on the order a map holds them in.
        left.sort_unstable_by_key(|(frame, at, _)| Reverse((*frame, *at)));
        for (_, _, id) in left.into_iter().skip(LEFT_AT_THE_END) {
            if let Some(drawn) = self.last.remove(&id)
                && self.now.get(&drawn.at) == Some(&id)
            {
                self.now.remove(&drawn.at);
            }
        }
    }
}

impl Store {
    /// Notes that the text known as `id` is drawn at `at` in this frame.
    pub(in super::super) fn placed(&mut self, id: &TextKey, at: TextAt) {
        let frame = self.frame;
        self.places.place(id, at, frame);
    }
}

impl Texts {
    /// Where the text known as `id` is now, if it's kept and has a place.
    pub(crate) fn place_of(&self, id: &TextKey) -> Option<TextAt> {
        self.0.borrow().places.of(id)
    }

    /// Where the text known as `id` was last drawn, if it's kept, whether or not another has taken its place since,
    /// which is where it's kept for.
    pub(crate) fn last_drawn_at(&self, id: &TextKey) -> Option<TextAt> {
        self.0.borrow().places.last_at(id)
    }
}

#[cfg(test)]
impl TextKey {
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
impl Texts {
    /// As if the text known as `id`, saying `text`, were drawn at `at`.
    pub(crate) fn drawn_as(&self, id: &TextKey, at: TextAt, text: &str) {
        self.0.borrow_mut().placed(id, at);
        self.parsed(id, text, at.of, super::super::ImagesAre::TheKits);
    }

    /// As if the text known by `at`, saying `text`, were drawn there.
    pub(crate) fn drawn_at(&self, at: TextAt, text: &str) {
        self.drawn_as(&TextKey::of(at), at, text);
    }

    /// What the text at `at` is known by, if one's there.
    pub(crate) fn known_at(&self, at: TextAt) -> Option<TextKey> {
        self.0.borrow().places.at(&at).cloned()
    }

    /// Whether the text known as `id` is kept, with what it was parsed as or what it shows.
    pub(crate) fn keeps(&self, id: &TextKey) -> bool {
        self.0.borrow().texts.contains_key(id)
    }
}
