//! How far what's in the strip along the window's top keeps clear of the window's buttons, and of the button beside
//! them that hides the sidebar on the left. On a Mac the buttons are macOS's own, in the top left corner, with that
//! button beside them. On Windows the app draws Windows' three at the strip's right end, and the button is alone in
//! the corner.

use gpui_kit::{App, Pixels, px};

use super::Sides;

/// How far macOS's buttons are asked to sit from the window's top and its left, which is also where the button beside
/// them starts without them.
pub(crate) const WINDOW_BUTTONS_INSET: Pixels = px(12.);
/// How wide macOS's three buttons are together: on macOS 27 each is 14px wide and they're 9px apart.
const MACOS_BUTTONS_WIDTH: Pixels = px(60.);
/// The space between macOS's buttons and the button beside them, which leaves that button's icon about as far from
/// them as they are from each other.
const AFTER_MACOS_BUTTONS: Pixels = px(6.);
/// The button that hides and shows the sidebar on the left: the kit's small button.
const TOGGLE_WIDTH: Pixels = px(24.);
/// How wide Windows' three buttons are together, minimize, maximize and close, each 46px as Windows draws its own.
pub(crate) const WINDOWS_BUTTONS_WIDTH: Pixels = px(46. * 3.);

/// Where the window's buttons are in the frame being drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WindowButtons {
    /// macOS's own, in the top left corner.
    InTheCorner,
    /// Windows' three, which the app draws at the right end of the strip.
    AtTheEnd,
    /// None in the strip: in full screen, where the system puts them away, and on Linux.
    #[default]
    Absent,
}

/// What reaches an end of the strip, which keeps clear of the window's buttons there in its own way.
#[derive(Clone, Copy, Debug)]
pub(crate) enum AtTheTop {
    /// The workspace's header, or Home's bar, beside both sidebars.
    Header,
    /// The top of the sidebar on the right, which reaches the window's right end.
    RightSidebar,
    /// What sits above the header and fills the strip, as an error does.
    AboveTheHeader,
}

/// How far something in the strip keeps from each end of the window: `start` from its left, and `end` from its right.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Insets {
    pub(crate) start: Pixels,
    pub(crate) end: Pixels,
}

/// What the strip keeps clear of in the frame being drawn: the window's buttons, and how much of each sidebar shows
/// over them.
#[derive(Clone, Copy, Default)]
pub(super) struct Clearance {
    buttons: WindowButtons,
    /// How much of each sidebar shows, left then right, from the window's edge.
    shown: (Pixels, Pixels),
    /// Whether something sits above the header and fills the strip, as an error does, so the header and the
    /// sidebar's top are below the window's buttons rather than beside them.
    above_the_header: bool,
}

impl Clearance {
    pub(super) fn new(buttons: WindowButtons, shown: (Pixels, Pixels), above_the_header: bool) -> Self {
        Self { buttons, shown, above_the_header }
    }

    /// How far what's at `at` keeps from each end of the window. What's beside a sidebar clears as much of what's at
    /// that end as the sidebar doesn't cover, which at the left is none while it's open, since even at its narrowest
    /// it covers the corner. The top of the sidebar on the right is under the buttons at that end whenever it shows,
    /// even as it slides in, when part of it is past the window's edge and passes under them.
    pub(super) fn of(self, at: AtTheTop) -> Insets {
        let beside = |room: Pixels, shown: Pixels| (room - shown).max(px(0.));
        let (start, end) = (self.start_room(), self.end_room());
        match at {
            AtTheTop::Header | AtTheTop::RightSidebar if self.above_the_header => Insets::default(),
            AtTheTop::Header => Insets { start: beside(start, self.shown.0), end: beside(end, self.shown.1) },
            AtTheTop::RightSidebar => Insets { start: px(0.), end },
            AtTheTop::AboveTheHeader => Insets { start: beside(start, self.shown.0), end },
        }
    }

    /// Where the button that hides and shows the sidebar on the left starts, from the window's left edge: after
    /// macOS's buttons where they're in the corner, and where they'd start otherwise.
    fn toggle_starts(self) -> Pixels {
        match self.buttons {
            WindowButtons::InTheCorner => WINDOW_BUTTONS_INSET + MACOS_BUTTONS_WIDTH + AFTER_MACOS_BUTTONS,
            WindowButtons::AtTheEnd | WindowButtons::Absent => WINDOW_BUTTONS_INSET,
        }
    }

    /// How much of the strip's left end the window's buttons and the button beside them take.
    fn start_room(self) -> Pixels {
        self.toggle_starts() + TOGGLE_WIDTH
    }

    /// How much of its right end the window's buttons take.
    fn end_room(self) -> Pixels {
        match self.buttons {
            WindowButtons::AtTheEnd => WINDOWS_BUTTONS_WIDTH,
            WindowButtons::InTheCorner | WindowButtons::Absent => px(0.),
        }
    }
}

/// How far what's at `at` keeps from each end of the window in the frame being drawn, to clear the window's buttons
/// and the button beside them.
pub(crate) fn clearance(at: AtTheTop, cx: &App) -> Insets {
    Sides::of(cx).placed.clearance.of(at)
}

/// Where the window's buttons are in the frame being drawn.
pub(crate) fn window_buttons(cx: &App) -> WindowButtons {
    Sides::of(cx).placed.clearance.buttons
}

/// Where the button that hides and shows the sidebar on the left starts in the frame being drawn, from the window's
/// left edge.
pub(crate) fn toggle_starts(cx: &App) -> Pixels {
    Sides::of(cx).placed.clearance.toggle_starts()
}
