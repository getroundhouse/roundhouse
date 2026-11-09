//! Where a workspace's work stands, in GitHub's colours, which the people Roundhouse is for already know. The mark's
//! fan and the pull request's bar draw from the same colours, so each means the same in both.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{App, Hsla};

use crate::palette::{faint, hex};

/// GitHub's, on a dark background.
const GREEN: u32 = 0x003f_b950;
const PURPLE: u32 = 0x00a3_71f7;
/// Muted, so a closed pull request doesn't read as the agent stopping short, which is the theme's red.
const RED: u32 = 0x00b9_605a;
const GREY: u32 = 0x008b_949e;
/// GitHub's for something waiting on GitHub, such as checks running or a place in the queue.
const YELLOW: u32 = 0x00d2_9922;
/// How strongly a pull request's colour shows through the bar behind it.
const TINT: f32 = 0.12;
/// How strongly it shows in the outline of a button that opens the pull request.
const EDGE: f32 = 0.35;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Work {
    NoPr,
    Draft,
    /// Open, with something to do before it can merge.
    Open,
    /// Open, with nothing in the way of merging it.
    Ready,
    /// In the merge queue, which merges it when its turn comes.
    Queued,
    Merged,
    Closed,
}

impl Work {
    /// The fan's colour, while nobody needs you. Every open pull request is green there, as GitHub shows it: a mark
    /// is too small to say more, and amber on it already means it's your turn.
    pub(crate) fn fan(self) -> Hsla {
        match self {
            Work::NoPr => faint(),
            Work::Draft => hex(GREY),
            Work::Open | Work::Ready | Work::Queued => hex(GREEN),
            Work::Merged => hex(PURPLE),
            Work::Closed => hex(RED),
        }
    }

    /// Its own colour in the bar, and the main action's fill, with the theme's dark text on it as every main action
    /// has. `None` for one that's either not a pull request yet or has something to do. Those two take the theme's,
    /// since neither is good or bad news.
    pub fn hue(self) -> Option<Hsla> {
        match self {
            Work::NoPr | Work::Open => None,
            Work::Draft => Some(hex(GREY)),
            Work::Ready => Some(hex(GREEN)),
            Work::Queued => Some(hex(YELLOW)),
            Work::Merged => Some(hex(PURPLE)),
            Work::Closed => Some(hex(RED)),
        }
    }

    /// What the bar says.
    pub fn text(self, cx: &App) -> Hsla {
        self.hue().unwrap_or(cx.theme().foreground)
    }

    /// The bar behind it. Something to do shows on a raised surface, so the bar still stands out from the sidebar.
    pub fn tint(self, cx: &App) -> Hsla {
        let theme = cx.theme();
        match (self, self.hue()) {
            (_, Some(hue)) => hue.opacity(TINT),
            (Work::Open, None) => theme.secondary,
            (_, None) => theme.sidebar,
        }
    }

    /// The outline of the button that opens the pull request.
    pub fn edge(self, cx: &App) -> Hsla {
        self.hue().map_or(cx.theme().input, |hue| hue.opacity(EDGE))
    }
}
