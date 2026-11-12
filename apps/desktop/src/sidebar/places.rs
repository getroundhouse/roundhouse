//! Where each of the sidebar's lines is going, so that when the order changes, or a line comes or goes, each line that
//! moves slides from where it was to where it now goes, rather than jumping. A line is laid out where it's going and
//! drawn where it has got to, so nothing else is laid out again for it as it moves.

use std::collections::HashMap;

use gpui_kit::{App, Pixels, Window, px};
use look::motion::Sliding;
use protocol::model::{RepoId, WorkspaceId};

/// What a line shows, which stays the same while its place in the list changes.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(super) enum LineId {
    Heading,
    Repo(RepoId),
    Workspace(WorkspaceId),
    /// The note in place of a new workspace on its way, of which there's only ever one.
    Creating,
}

/// Each line's place, kept by the app rather than the list, which is made again for each frame.
#[derive(Default)]
pub(super) struct Places {
    lines: HashMap<LineId, Place>,
}

struct Place {
    /// How far down the list it goes.
    top: Pixels,
    /// How far down it has got on the way there.
    now: Sliding,
}

impl Places {
    /// Sends each line to its top, from wherever it has got to, so one turned back partway doesn't jump. A line that
    /// wasn't there before is at its top at once, and one that has gone is forgotten, so one that comes back later
    /// doesn't slide in from where it was then.
    pub(super) fn go(&mut self, tops: impl IntoIterator<Item = (LineId, Pixels)>, cx: &App) {
        let mut placed = HashMap::with_capacity(self.lines.len());
        for (id, top) in tops {
            let place = match self.lines.remove(&id) {
                Some(mut place) if place.top != top => {
                    place.now.go(f32::from(top), cx);
                    place.top = top;
                    place
                }
                Some(place) => place,
                None => Place { top, now: Sliding::at(f32::from(top)) },
            };
            placed.insert(id, place);
        }
        self.lines = placed;
    }

    /// How far below its top to draw a line now, which is above it for one on its way up. Asks for the next frame
    /// while it moves.
    pub(super) fn offset(&self, id: &LineId, window: &mut Window, cx: &App) -> Pixels {
        self.lines.get(id).map_or(px(0.), |place| px(place.now.draw(window, cx)) - place.top)
    }
}
