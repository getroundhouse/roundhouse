//! Which text is beside another in the chat's order, for a key that moves the end of what's selected into the next, and
//! which is first in what one place draws, for a key that moves it to the start of a prompt, a heading or a part.

use std::ops::Bound;

use super::{Leaves, TextAt, TextKey, Texts};
use crate::selection::Way;

impl Texts {
    /// The nearest text to `at` that's `way` of it in the chat's order, that isn't hidden and shows something: what
    /// it's known by and what it shows. One with nothing parsed yet shows nothing, so it isn't it.
    pub(crate) fn beside(&self, at: TextAt, way: Way) -> Option<(TextKey, Leaves)> {
        let store = self.0.borrow();
        let found = |(_, id): (&TextAt, &TextKey)| {
            let leaves = store.texts.get(id).filter(|_| !store.hidden.contains(id))?.leaves()?;
            leaves.last_leaf().is_some().then(|| (id.clone(), leaves))
        };
        match way {
            Way::On => store.places.within((Bound::Excluded(at), Bound::Unbounded)).find_map(found),
            Way::Back => store.places.within(..at).rev().find_map(found),
        }
    }

    /// The first text at `at` or after it that what `at` is in draws, that isn't hidden and shows something, if one
    /// has been drawn there.
    pub(crate) fn first_at(&self, at: TextAt) -> Option<TextKey> {
        let store = self.0.borrow();
        let shows = |id: &TextKey| {
            let leaves =
                store.texts.get(id).filter(|_| !store.hidden.contains(id)).and_then(super::cache::Text::leaves);
            leaves.is_some_and(|leaves| leaves.last_leaf().is_some())
        };
        let from = store.places.within((Bound::Included(at), Bound::Unbounded));
        from.take_while(|(place, _)| place.of == at.of).find(|(_, id)| shows(id)).map(|(_, id)| id.clone())
    }
}
