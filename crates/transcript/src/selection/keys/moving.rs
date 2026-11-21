//! Moving the end of what's selected that moved last by a key: a character, a word or a line, as the key bound to it
//! says, from one text into the next as a drag goes, and what's kept for the end it moved.

use gpui_kit::base::actions::{SelectDown, SelectLeft, SelectRight, SelectUp};
use gpui_kit::base::input::{SelectToNextWordEnd, SelectToPreviousWordStart};
use gpui_kit::{Action, App, Div, InteractiveElement as _, Window};
use unicode_segmentation::UnicodeSegmentation as _;

use super::super::places::in_a_word;
use super::super::{End, Selection};
use super::lines::Run;
use super::sight::ToDo;
use crate::texts::{Leaves, TextKey};

/// Which way a key goes, moving the end of what's selected or stepping a view: back, which is left or up, or on, which
/// is right or down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    Back,
    On,
}

/// How far a key moves the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    /// A character, as a person sees one.
    Character(Way),
    /// To the start of the word before, or the end of the word after.
    Word(Way),
    /// To the line above or below.
    Line(Way),
}

/// What the keys that move an end keep: each thing for the end it was for, so once anything else moves that end, it's
/// left behind, and nothing else has to let go of it.
#[derive(Default)]
pub(in crate::selection) struct Keyed {
    /// The run of moves up and down that left the end where it is.
    pub(super) run: Option<Run>,
    /// What's still to do for the end the keys moved last.
    pub(super) to_do: Option<ToDo>,
    /// The end the view's `Reveal` is bringing into view, once a key has moved it, which the selection says where it's
    /// drawn.
    pub(super) revealing: Option<End>,
}

/// What came of trying a move.
#[derive(Debug, PartialEq)]
pub(super) enum Tried {
    /// It moved the end here, in a run of moves up and down if it was one.
    Moved(End, Option<Run>),
    /// What it goes to is past the texts drawn, this way.
    Past(Way),
    /// The end isn't drawn, and has to be, to know where it goes.
    NotDrawn,
    /// There's nowhere to go.
    Nowhere,
}

impl Selection {
    /// `view`, with the keys that move the end of what's selected that moved last, which are the actions a text box
    /// takes to move the end of what's selected in it.
    pub(in crate::selection) fn with_moving_keys(&self, view: Div) -> Div {
        (view.on_action(self.moving::<SelectLeft>(Step::Character(Way::Back))))
            .on_action(self.moving::<SelectRight>(Step::Character(Way::On)))
            .on_action(self.moving::<SelectToPreviousWordStart>(Step::Word(Way::Back)))
            .on_action(self.moving::<SelectToNextWordEnd>(Step::Word(Way::On)))
            .on_action(self.moving::<SelectUp>(Step::Line(Way::Back)))
            .on_action(self.moving::<SelectDown>(Step::Line(Way::On)))
    }

    /// What the key bound to `A` does, which is to move the end by `step`.
    fn moving<A: Action>(&self, step: Step) -> impl Fn(&A, &mut Window, &mut App) + 'static {
        let selection = self.clone();
        move |_, window, cx| selection.step(step, window, cx)
    }

    /// Moves the end of what's selected that moved last by `step`, if anything's selected, or a press has left a place
    /// to start from, and otherwise leaves the key to whatever else takes it. What it moves to is brought into view,
    /// and a move that has to wait for what's drawn goes on once it has been, as `sight` says.
    pub(super) fn step(&self, step: Step, window: &mut Window, cx: &mut App) {
        let ends = self.inner.borrow().ends.clone();
        let Some((anchor, far)) = ends.and_then(|(anchor, far)| Some((anchor, self.in_its_text(&far)?))) else {
            cx.propagate();
            return;
        };
        let mut selecting = self.inner.borrow_mut();
        // A key held down repeats while what it moved to waits for what scrolls the texts to scroll, and goes on with
        // that.
        if selecting.keyed.to_do.as_ref().is_some_and(|to_do| to_do.waits_to(&far, step)) {
            return;
        }
        // As a place in what its text shows now, so a move that waits finds it where it is.
        selecting.set(Some((anchor, far.clone())));
        selecting.drag = None;
        selecting.keyed.to_do = Some(ToDo::moving(far, step));
        drop(selecting);
        // What's left to do, what scrolls the texts does as the next frame starts.
        self.go_on_by_keys();
        window.refresh();
    }

    /// Tries moving `end` by `step`, by what's drawn and the texts held now, with `past` saying whether what scrolls
    /// the texts has scrolled past those that were drawn when it was first tried, which can leave the end undrawn.
    pub(super) fn try_move(&self, end: &End, step: Step, past: bool) -> Tried {
        let Some(leaves) = self.texts.leaves_of(&end.text) else { return Tried::Nowhere };
        let Some(text) = leaves.leaf(end.leaf) else { return Tried::Nowhere };
        let (way, beside) = match step {
            Step::Character(way) => (way, character_beside(text, end.byte, way)),
            Step::Word(way) => (way, word_beside(text, end.byte, way)),
            Step::Line(way) => return self.line_beside(end, way, past),
        };
        if let Some(byte) = beside {
            return Tried::Moved(End { byte, ..end.clone() }, None);
        }
        let (edge, leaves) = match self.across_the_edge(end, &leaves, way, past) {
            Ok(edge) => edge,
            Err(tried) => return tried,
        };
        // From the edge of a paragraph, or of any other leaf, a word goes to the start of the last word of the one
        // before or the end of the first word of the one after, or that leaf's edge if it has none, as in any text.
        let word = matches!(step, Step::Word(_)).then(|| word_beside(leaves.leaf(edge.leaf)?, edge.byte, way));
        Tried::Moved(word.flatten().map_or(edge.clone(), |byte| End { byte, ..edge }), None)
    }

    /// The edge of the leaf beside the one `end` is at the edge of, `way`, in a text that shows `leaves`: the start of
    /// the next leaf, or the end of the one before, in its text or the text beside it, with what that text shows.
    ///
    /// Into another text, it goes only from a text that's drawn to one that's drawn, since the rows between two that
    /// are drawn are drawn too, so it doesn't pass a text in a row that's never been drawn, which isn't held. With the
    /// other not drawn, it's past what's drawn, and with the end's not drawn, the end has to be first, unless what
    /// scrolls the texts has gone `past` it for this move.
    fn across_the_edge(&self, end: &End, leaves: &Leaves, way: Way, past: bool) -> Result<(End, Leaves), Tried> {
        let last = leaves.last_leaf().ok_or(Tried::Nowhere)?;
        let in_this = match way {
            Way::Back => end.leaf.checked_sub(1),
            Way::On => (end.leaf < last).then_some(end.leaf + 1),
        };
        let (text, leaf, leaves) = if let Some(leaf) = in_this {
            (end.text.clone(), leaf, leaves.clone())
        } else {
            let drawn = |text: &TextKey| self.inner.borrow().drawn.iter().any(|drawn| drawn.text == *text);
            if !past && !drawn(&end.text) {
                return Err(Tried::NotDrawn);
            }
            let at = self.texts.place_of(&end.text).ok_or(Tried::Nowhere)?;
            let (text, leaves) = self.texts.beside(at, way).filter(|(text, _)| drawn(text)).ok_or(Tried::Past(way))?;
            let leaf = match way {
                Way::Back => leaves.last_leaf().ok_or(Tried::Nowhere)?,
                Way::On => 0,
            };
            (text, leaf, leaves)
        };
        let byte = match way {
            Way::Back => leaves.leaf(leaf).map_or(0, str::len),
            Way::On => 0,
        };
        Ok((End { text, leaf, byte }, leaves))
    }

    /// Moves the end that moved last to `end`, in `run` if a run of moves up and down took it there, and has it
    /// brought into view.
    pub(super) fn moved_to(&self, end: End, run: Option<Run>) {
        {
            let mut selecting = self.inner.borrow_mut();
            let Some((anchor, _)) = selecting.ends.clone() else { return };
            selecting.set(Some((anchor, end.clone())));
            selecting.keyed.run = run;
        }
        self.bring_into_view(end);
    }

    /// `end`, as a place in what its text shows now, if it's known. A text can show less than it did when the end was
    /// put there, once it's been parsed again, and an end can be past the end of its text, as selecting everything puts
    /// it.
    fn in_its_text(&self, end: &End) -> Option<End> {
        let leaves = self.texts.leaves_of(&end.text)?;
        let last = leaves.last_leaf()?;
        let (leaf, byte) = if end.leaf > last { (last, usize::MAX) } else { (end.leaf, end.byte) };
        let text = leaves.leaf(leaf)?;
        let byte = text.floor_char_boundary(byte.min(text.len()));
        Some(End { text: end.text.clone(), leaf, byte })
    }
}

/// The place a character `way` of `byte` in `text`, a character being what a person sees as one, such as a letter
/// with its accent, or `None` at its edge.
pub(super) fn character_beside(text: &str, byte: usize, way: Way) -> Option<usize> {
    match way {
        Way::Back => text[..byte].graphemes(true).next_back().map(|before| byte - before.len()),
        Way::On => text[byte..].graphemes(true).next().map(|after| byte + after.len()),
    }
}

/// The place at the start of the word before `byte` in `text`, or the end of the word after it, past what isn't in a
/// word between, or `None` at its edge. A place in a word goes to that word's start or end. With no word that way, it's
/// the text's edge.
pub(super) fn word_beside(text: &str, byte: usize, way: Way) -> Option<usize> {
    let moved = match way {
        Way::Back => {
            let before = &text[..byte];
            let gap = before.chars().rev().take_while(|c| !in_a_word(*c)).map(char::len_utf8).sum::<usize>();
            let word = before[..byte - gap].chars().rev().take_while(|c| in_a_word(*c)).map(char::len_utf8);
            byte - gap - word.sum::<usize>()
        }
        Way::On => {
            let after = &text[byte..];
            let gap = after.chars().take_while(|c| !in_a_word(*c)).map(char::len_utf8).sum::<usize>();
            let word = after[gap..].chars().take_while(|c| in_a_word(*c)).map(char::len_utf8);
            byte + gap + word.sum::<usize>()
        }
    };
    (moved != byte).then_some(moved)
}
