//! Where each of a list's rows is going, so that when the order changes, or a row comes or goes, each row that moves
//! slides from where it was to where it now goes. It says where each row is drawn in this frame, and
//! [`list`](super::list) draws it there.

use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

use gpui_kit::{App, Pixels, Window, px};
use look::motion::Sliding;

/// Each row's place, by what the row shows, `K`, which stays the same while its place in the list changes. Kept by
/// the app rather than the list, which is made again for each frame.
pub(crate) struct Places<K> {
    rows: HashMap<K, Place>,
}

impl<K> Default for Places<K> {
    fn default() -> Self {
        Self { rows: HashMap::new() }
    }
}

/// Where a list's rows are in this frame, as [`Places::arrange`] sent them.
pub(crate) struct Arrangement {
    /// How far down the list each row is drawn, from the top down: where it has got to on its way to its place.
    pub(crate) drawn: Rc<[Pixels]>,
    /// How far down the list ends, below the last row's place.
    pub(crate) end: Pixels,
}

struct Place {
    /// How far down the list it goes.
    top: Pixels,
    /// How far down it has got on the way there.
    now: Sliding,
}

impl<K: Clone + Eq + Hash> Places<K> {
    /// Sends each row, given from the top down with the room it takes, to its place, from wherever it has got to, so
    /// one turned back partway doesn't jump. Says where to draw each now, and asks for the next frame while one moves.
    /// A row that wasn't there before is at its place at once, and one that has gone is forgotten, so one that comes
    /// back later doesn't slide in from where it was then.
    pub(crate) fn arrange(
        &mut self,
        rows: impl IntoIterator<Item = (K, Pixels)>,
        window: &mut Window,
        cx: &App,
    ) -> Arrangement {
        let (mut top, mut drawn) = (px(0.), Vec::new());
        let mut placed = HashMap::with_capacity(self.rows.len());
        for (id, height) in rows {
            let place = match self.rows.remove(&id) {
                Some(mut place) if place.top != top => {
                    place.now.go(f32::from(top), cx);
                    place.top = top;
                    place
                }
                Some(place) => place,
                None => Place { top, now: Sliding::at(f32::from(top)) },
            };
            drawn.push(px(place.now.draw(window, cx)));
            placed.insert(id, place);
            top += height;
        }
        self.rows = placed;
        Arrangement { drawn: drawn.into(), end: top }
    }

    /// Forgets every row, so each is at its place at once the next time the list is drawn.
    pub(crate) fn forget(&mut self) {
        self.rows.clear();
    }
}
