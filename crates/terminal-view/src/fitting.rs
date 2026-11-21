//! The size a view gives its terminal: as many cells as fit in its room, once the room has stopped changing.
use protocol::terminal::TerminalSize;

/// Whether the room a terminal's view has is still changing, as it is on each frame of a sidebar's slide beside it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Room {
    Settled,
    Moving,
}

/// The size a view last gave its terminal, which it gives again only once it has room for another.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Fitting {
    given: Option<TerminalSize>,
}

impl Fitting {
    /// The size to give the terminal with room for `fits` in a room that's `room`, if there's one to give. None while
    /// the room is moving, since each size given resizes the program, and the worker and every client showing the
    /// terminal reflow it, so a slide would resize it on every frame. Until the room settles, the view draws the
    /// terminal at the size it has, cut off where the room is smaller, with the rest of its room empty where it's
    /// bigger. None for the size given last either, which the terminal already has.
    pub(crate) fn give(&mut self, fits: TerminalSize, room: Room) -> Option<TerminalSize> {
        match room {
            Room::Moving => None,
            Room::Settled if self.given == Some(fits) => None,
            Room::Settled => {
                self.given = Some(fits);
                self.given
            }
        }
    }

    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn given(self) -> Option<TerminalSize> {
        self.given
    }
}

#[cfg(test)]
mod tests {
    use protocol::terminal::TerminalSize;

    use super::{Fitting, Room};

    const NARROW: TerminalSize = TerminalSize { cols: 80, rows: 24 };
    const WIDE: TerminalSize = TerminalSize { cols: 120, rows: 24 };

    /// A view first laid out partway through a slide gives none until it ends, and the terminal keeps the size it
    /// opened at meanwhile.
    #[test]
    fn a_size_is_given_once_the_room_settles_and_only_when_its_new() {
        let mut fitting = Fitting::default();

        assert_eq!(fitting.give(NARROW, Room::Moving), None, "laid out first while moving");
        assert_eq!(fitting.give(NARROW, Room::Settled), Some(NARROW), "settled");
        assert_eq!(fitting.give(NARROW, Room::Settled), None, "the same again");
        assert_eq!(fitting.give(TerminalSize { cols: 100, ..NARROW }, Room::Moving), None, "on the way");
        assert_eq!(fitting.give(WIDE, Room::Moving), None, "nearly there");
        assert_eq!(fitting.give(WIDE, Room::Settled), Some(WIDE), "there");
        assert_eq!(fitting.given(), Some(WIDE));
    }
}
