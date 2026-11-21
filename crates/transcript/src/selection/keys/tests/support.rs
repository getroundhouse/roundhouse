//! What the keys' tests share: a selection with a window to press keys in, a stand-in for what scrolls the texts that
//! says what it was asked to do, and a frame drawn with it.

use std::cell::{Cell, RefCell};
use std::time::Instant;

use gpui_kit::{Bounds, Pixels, TestAppContext, VisualTestContext, point, px, size};

use super::super::super::{End, Scroller, Selection};
use super::super::Step;
use crate::texts::{Of, TextKey, Texts};

/// A selection over the texts `texts` holds, with a window to press keys in.
pub(super) fn selecting<'a>(cx: &'a mut TestAppContext, texts: &Texts) -> (Selection, &'a mut VisualTestContext) {
    let selection = Selection::new(texts.clone(), cx.update(|cx| cx.focus_handle()));
    (selection, cx.add_empty_window())
}

/// What the stand-in for what scrolls the texts was asked to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Asked {
    /// Scroll down by this, or up for less than nothing.
    Down(Pixels),
    /// Go to where this is drawn.
    Show(Of),
}

/// What scrolls the texts, which notes what it's asked to do, and does it, until it's `at_the_end`, where a scroll
/// moves nothing. While its rows all `show`, there's nowhere to go to.
#[derive(Default)]
pub(super) struct Scrolling {
    pub(super) asked: RefCell<Vec<Asked>>,
    pub(super) scrolled: Cell<Pixels>,
    pub(super) at_the_end: Cell<bool>,
    pub(super) shows: bool,
}

impl Scroller for Scrolling {
    fn scroll_down(&self, by: Pixels) {
        self.asked.borrow_mut().push(Asked::Down(by));
        if !self.at_the_end.get() {
            self.scrolled.set(self.scrolled.get() + by);
        }
    }

    fn scrolled(&self) -> Pixels {
        self.scrolled.get()
    }

    fn show(&self, of: Of) -> bool {
        self.asked.borrow_mut().push(Asked::Show(of));
        if self.shows {
            return false;
        }
        self.scrolled.set(self.scrolled.get() + px(1.));
        true
    }

    /// The keys that move an end never go to the start, nor the end.
    fn to_start(&self) {
        unreachable!("only the keys that scroll a page go to the start");
    }

    fn to_end(&self, _: Instant) {
        unreachable!("only the keys that scroll a page go to the end");
    }
}

/// A frame of `scrolling` showing what's in `bounds`, as the element that holds it has it: it scrolls as the frame
/// starts, `draw` draws the texts, and the keys go on from those. Says whether it asks for the next frame.
pub(super) fn frame(selection: &Selection, scrolling: &Scrolling, bounds: Bounds<Pixels>, draw: impl FnOnce()) -> bool {
    let started = selection.frame_started(bounds, scrolling, Instant::now());
    draw();
    selection.texts_laid_out(bounds, scrolling, started)
}

/// What a window 200 square shows.
pub(super) fn a_window() -> Bounds<Pixels> {
    Bounds::new(point(px(0.), px(0.)), size(px(200.), px(200.)))
}

/// Moves the end that moved last of what `selection` has selected by `step`, as its key does.
pub(super) fn step(selection: &Selection, window: &mut VisualTestContext, step: Step) {
    window.update(|window, cx| selection.step(step, window, cx));
}

pub(super) fn place(text: &TextKey, leaf: u64, byte: usize) -> End {
    End { text: text.clone(), leaf, byte }
}
