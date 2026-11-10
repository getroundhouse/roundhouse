//! When the outline's list shows: once the pointer has rested on the ticks, and until it has left the list, which
//! opens over them, for long enough.

use std::time::Duration;

use gpui_kit::{Context, Task};

/// Long enough that the pointer passing over the ticks on its way somewhere else doesn't open the list.
const OPEN_DELAY: Duration = Duration::from_millis(150);
/// Long enough that the pointer slipping off the list's edge for a moment doesn't close it, and that the frame where
/// the list has covered the ticks but not yet heard the pointer is over it doesn't either.
const CLOSE_DELAY: Duration = Duration::from_millis(200);

/// Whether the list shows, and what the pointer is over. It's the outline's element state, which GPUI keeps only while
/// the outline is drawn, so a chat that stops showing without the pointer leaving, such as one another client closes,
/// forgets an open list rather than showing it again with the pointer long gone.
#[derive(Default)]
pub(super) struct Hover {
    showing: Showing,
    over_ticks: bool,
    over_list: bool,
    /// The pointer is over the ticks without having moved to them: a prompt was picked from the list that covered
    /// them, or the wheel is scrolling the chat, which doesn't move the pointer. They open the list only once it moves.
    needs_move: bool,
    /// Opens or closes the list once the pointer has stayed, or stayed away, long enough.
    waiting: Option<Task<()>>,
}

/// Whether the list shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Showing {
    #[default]
    Closed,
    /// Opened since the last frame, so this one scrolls it to the prompt being read.
    Opened,
    Open,
}

/// Which part of the outline the pointer is over.
#[derive(Clone, Copy, Debug)]
pub(super) enum Part {
    Ticks,
    List,
}

impl Hover {
    /// Whether the list shows this frame. Only the first frame it's open is [`Showing::Opened`].
    pub(super) fn frame(&mut self) -> Showing {
        let showing = self.showing;
        if showing == Showing::Opened {
            self.showing = Showing::Open;
        }
        showing
    }

    pub(super) fn hovered(&mut self, part: Part, hovered: bool, cx: &mut Context<Self>) {
        match part {
            Part::Ticks => self.over_ticks = hovered,
            Part::List => self.over_list = hovered,
        }
        if matches!((part, hovered), (Part::Ticks, false)) {
            self.needs_move = false;
        }
        self.want(cx);
    }

    /// The pointer moved over the ticks, so it isn't only resting where a picked prompt or the wheel left it.
    pub(super) fn moved_over_ticks(&mut self, cx: &mut Context<Self>) {
        if self.needs_move {
            self.needs_move = false;
            self.want(cx);
        }
    }

    /// The wheel turned. Over the ticks, it scrolls the chat under them, so the pointer resting there while it does
    /// isn't asking for the list.
    pub(super) fn wheeled(&mut self, cx: &mut Context<Self>) {
        if self.over_ticks && !self.needs_move {
            self.needs_move = true;
            self.want(cx);
        }
    }

    /// Opens or closes the list, once it's waited, to match where the pointer is.
    fn want(&mut self, cx: &mut Context<Self>) {
        let wanted = (self.over_ticks && !self.needs_move) || self.over_list;
        if wanted == (self.showing != Showing::Closed) {
            self.waiting = None;
            return;
        }
        let delay = if wanted { OPEN_DELAY } else { CLOSE_DELAY };
        self.waiting = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |hover, cx| hover.set_open(wanted, cx));
        }));
    }

    /// The list goes once a prompt is picked, and the pointer it was under is no longer over the list. It doesn't open
    /// again until the pointer moves over the ticks.
    pub(super) fn jumped(&mut self, cx: &mut Context<Self>) {
        self.over_list = false;
        self.needs_move = true;
        self.set_open(false, cx);
    }

    fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.waiting = None;
        self.showing = match (open, self.showing) {
            (false, _) => Showing::Closed,
            (true, Showing::Closed) => Showing::Opened,
            (true, showing) => showing,
        };
        cx.notify();
    }
}
