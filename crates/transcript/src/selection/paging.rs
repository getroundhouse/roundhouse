//! The keys that scroll a view of the selection's texts, a chat, a rendered Markdown file or a proposal, as they'd
//! scroll a page: Up and Down a line, Page Up and Page Down a screen, and Home and End to its start and its end. They're
//! bound to the actions a text box moves its cursor with, as the keys that move an end of what's selected are, since a
//! view of the texts has no cursor, and moving one there is scrolling. A key asks, and what scrolls the texts does it
//! as the next frame starts, as `frames` says, since that's when how much of the view is in sight is known, and how far
//! its start and its end are is each view's own, as `Scroller` says.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use gpui_kit::base::input::{MoveDown, MovePageDown, MovePageUp, MoveToEnd, MoveToStart, MoveUp};
use gpui_kit::{Action, App, Bounds, Div, InteractiveElement as _, Pixels, Window};

use super::{Scroller, Way};

/// How much of what's in sight a screen scrolls, leaving an eighth of it in sight, so what was at its edge still shows,
/// as a browser's Page Down leaves it.
const SCREEN: f32 = 0.875;

/// What the keys have asked of what scrolls the texts since the last frame started.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Asked {
    /// Down by this, or up for less than nothing, which is a line at a time.
    By(Pixels),
    /// Down this many screens, or up for fewer than none.
    Screens(f32),
    /// To the start, or on to the end.
    To(Way),
}

/// What the keys have asked of the view, which keys pressed between two frames add to, if they're the same kind of
/// move, as a key held down repeating can be. Cheap to clone.
#[derive(Clone, Default)]
pub(crate) struct Paging(Rc<Cell<Option<Asked>>>);

impl Paging {
    /// Asks for `asked`, after what was asked before, which it adds to if they're the same kind of move, and replaces
    /// if they aren't, since going to an end leaves nothing of a line before.
    pub(crate) fn ask(&self, asked: Asked) {
        let both = match (self.0.get(), asked) {
            (Some(Asked::By(before)), Asked::By(by)) => Asked::By(before + by),
            (Some(Asked::Screens(before)), Asked::Screens(screens)) => Asked::Screens(before + screens),
            (_, asked) => asked,
        };
        self.0.set(Some(both));
    }

    /// Scrolls `scroller`, which shows the texts in `in_view` as a frame starts at `now`, as the keys asked, saying
    /// whether they had.
    pub(super) fn scroll(&self, scroller: &dyn Scroller, in_view: Bounds<Pixels>, now: Instant) -> bool {
        let Some(asked) = self.0.take() else { return false };
        match asked {
            Asked::By(by) => scroller.scroll_down(by),
            Asked::Screens(screens) => scroller.scroll_down(in_view.size.height * SCREEN * screens),
            Asked::To(Way::Back) => scroller.to_start(),
            Asked::To(Way::On) => scroller.to_end(now),
        }
        true
    }

    /// `view`, with the keys that scroll it, which ask what scrolls its texts to. A line is the window's, as the wheel
    /// turns one.
    pub(super) fn with_keys(&self, view: Div) -> Div {
        (view.on_action(self.asking::<MoveUp>(|window| Asked::By(-window.line_height()))))
            .on_action(self.asking::<MoveDown>(|window| Asked::By(window.line_height())))
            .on_action(self.asking::<MovePageUp>(|_| Asked::Screens(-1.)))
            .on_action(self.asking::<MovePageDown>(|_| Asked::Screens(1.)))
            .on_action(self.asking::<MoveToStart>(|_| Asked::To(Way::Back)))
            .on_action(self.asking::<MoveToEnd>(|_| Asked::To(Way::On)))
    }

    /// What the key bound to `A` does, which is to ask for what `asked` says, and draw the window again for it.
    fn asking<A: Action>(&self, asked: fn(&Window) -> Asked) -> impl Fn(&A, &mut Window, &mut App) + 'static {
        let paging = self.clone();
        move |_, window, _| {
            paging.ask(asked(window));
            window.refresh();
        }
    }
}
