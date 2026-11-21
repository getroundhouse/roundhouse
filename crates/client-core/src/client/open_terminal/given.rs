//! What views give a terminal's window, held until the thread it's followed on looks, which can be a while: that thread
//! waits on the hub, as for the reply to a watch, or to write while the hub reads nothing.

use protocol::terminal::{KeyPress, TerminalSize};

use super::window::{Pointer, Replied, SelectBy};
use crate::drawn::Rgb;

/// What a view gives the window, which only the thread it's followed on reaches, and the replies that thread waits for
/// elsewhere.
pub(super) enum Given {
    Key(KeyPress),
    Paste(String),
    Text(String),
    /// Turns the wheel by this many lines, up for fewer than none.
    Wheel(isize),
    /// The view has room for this size.
    Fit(TerminalSize),
    Colors(Rgb, Rgb),
    /// The pointer's button went down, to select by what the click of a run it was selects by.
    Press(Pointer, SelectBy),
    /// The pointer moved with the button down, since a press on the grid.
    Drag(Pointer),
    Release,
    SelectAll,
    /// What's selected, as text, for the view to copy. Nothing is sent while nothing is.
    SelectedText(flume::Sender<String>),
    Replied(Replied),
}

/// What views gave the window since its thread last looked, and the replies to what it asked, oldest first.
///
/// A run of drags, of sizes or of colours is held as its newest, since the window takes each in place of the one before.
/// So a pointer dragged while the thread waits on a hub that doesn't answer holds one drag, not one for each move.
/// Everything else is held, in order, and what's given on either side of it stays on its side, since a key pressed
/// after a new size has to reach a program that has it. A turn of the wheel is held too, since a full-screen program
/// reads each line of it as an arrow key, and the worker sends at most a screen's worth for one turn.
#[derive(Default)]
pub(super) struct Queue(Vec<Given>);

impl Queue {
    pub(super) fn give(&mut self, given: Given) {
        let taken_in = match self.0.last_mut() {
            Some(last) => last.take_in(given),
            None => Err(given),
        };
        if let Err(given) = taken_in {
            self.0.push(given);
        }
    }

    /// Everything held, oldest first, leaving nothing.
    pub(super) fn take(&mut self) -> Vec<Given> {
        std::mem::take(&mut self.0)
    }
}

impl Given {
    /// Takes in `newer`, given just after this, where the two do as one what they'd do one after the other, or gives it
    /// back.
    fn take_in(&mut self, newer: Given) -> Result<(), Given> {
        match (self, newer) {
            (Given::Drag(at), Given::Drag(newer)) => *at = newer,
            (Given::Fit(room), Given::Fit(newer)) => *room = newer,
            (Given::Colors(foreground, background), Given::Colors(newer_foreground, newer_background)) => {
                (*foreground, *background) = (newer_foreground, newer_background);
            }
            (
                _,
                newer @ (Given::Drag(_)
                | Given::Fit(_)
                | Given::Colors(..)
                | Given::Wheel(_)
                | Given::Key(_)
                | Given::Paste(_)
                | Given::Text(_)
                | Given::Press(..)
                | Given::Release
                | Given::SelectAll
                | Given::SelectedText(_)
                | Given::Replied(_)),
            ) => return Err(newer),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};
    use protocol::terminal::{Key, Mods};

    use super::*;

    /// What a view gives, in a form a test can compare, of the kinds these tests give.
    #[derive(Clone, Debug, PartialEq)]
    enum Gave {
        Key(char),
        Paste(String),
        Wheel(isize),
        Fit(u16),
        Colors(u8),
        Drag(u16),
        Release,
    }

    impl Gave {
        fn given(&self) -> Given {
            let gray = |level| Rgb { r: level, g: level, b: level };
            let mods = Mods::default();
            match self.clone() {
                Gave::Key(typed) => {
                    Given::Key(KeyPress { key: Key::Char(typed), mods, text: Some(typed.into()), consumed: mods })
                }
                Gave::Paste(text) => Given::Paste(text),
                Gave::Wheel(lines) => Given::Wheel(lines),
                Gave::Fit(cols) => Given::Fit(TerminalSize { cols, rows: 24 }),
                Gave::Colors(level) => Given::Colors(gray(level), gray(255 - level)),
                Gave::Drag(col) => Given::Drag(Pointer { col: f32::from(col), row: 0. }),
                Gave::Release => Given::Release,
            }
        }

        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "made from a u16 by `given`")]
        fn of(given: &Given) -> Gave {
            match given {
                Given::Key(press) => Gave::Key(press.text.as_deref().and_then(|text| text.chars().next()).unwrap()),
                Given::Paste(text) => Gave::Paste(text.clone()),
                Given::Wheel(lines) => Gave::Wheel(*lines),
                Given::Fit(size) => Gave::Fit(size.cols),
                Given::Colors(foreground, _) => Gave::Colors(foreground.r),
                Given::Drag(at) => Gave::Drag(at.col as u16),
                Given::Release => Gave::Release,
                Given::Text(_) | Given::Press(..) | Given::SelectAll | Given::SelectedText(_) | Given::Replied(_) => {
                    unreachable!("these tests give none")
                }
            }
        }

        /// Whether only the newest of a run of these matters.
        fn newest_only(&self) -> bool {
            matches!(self, Gave::Fit(_) | Gave::Colors(_) | Gave::Drag(_))
        }
    }

    fn gave() -> impl Strategy<Value = Gave> {
        prop_oneof![
            any::<char>().prop_map(Gave::Key),
            "[a-z]{0,3}".prop_map(Gave::Paste),
            (-3_isize..=3).prop_map(Gave::Wheel),
            (1_u16..4).prop_map(Gave::Fit),
            (0_u8..4).prop_map(Gave::Colors),
            (0_u16..4).prop_map(Gave::Drag),
            Just(Gave::Release),
        ]
    }

    /// What's held once each of `gave` is given, in turn.
    fn held(gave: &[Gave]) -> Vec<Gave> {
        let mut queue = Queue::default();
        for gave in gave {
            queue.give(gave.given());
        }
        queue.take().iter().map(Gave::of).collect()
    }

    /// `gave` in parts, each ending in one of what's always held, if it isn't the last: that one, and the newest size,
    /// colours and drag given before it in its part.
    fn parts(gave: &[Gave]) -> Vec<(Option<Gave>, [Option<Gave>; 3])> {
        let part = |part: &[Gave]| {
            let newest = |kind: fn(&Gave) -> bool| part.iter().rev().find(|gave| kind(gave)).cloned();
            let always = part.last().filter(|gave| !gave.newest_only()).cloned();
            let newest = [
                newest(|gave| matches!(gave, Gave::Fit(_))),
                newest(|gave| matches!(gave, Gave::Colors(_))),
                newest(|gave| matches!(gave, Gave::Drag(_))),
            ];
            (always, newest)
        };
        gave.split_inclusive(|gave| !gave.newest_only()).map(part).collect()
    }

    proptest! {
        #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

        /// Every key, paste, turn of the wheel and release is held, in order. Between two of them, the newest size,
        /// colours and drag given there are held there, and no two sizes, colours or drags are held one after the
        /// other.
        #[test]
        fn a_run_of_what_only_its_newest_matters_for_is_held_as_one(gave in vec(gave(), 0..40)) {
            let held = held(&gave);

            prop_assert_eq!(parts(&held), parts(&gave));
            for pair in held.windows(2) {
                let same = std::mem::discriminant(&pair[0]) == std::mem::discriminant(&pair[1]);
                prop_assert!(!(same && pair[0].newest_only()), "{held:?}");
            }
        }
    }
}
