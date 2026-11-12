//! Where the window's buttons sit, and how far what's in the strip along the window's top keeps clear of them. On a
//! Mac they're macOS's own, in the top left corner, with the button that hides the sidebar on the left beside them.
//! On Windows the app draws Windows' three at the strip's right end.

use gpui_kit::{App, IntoElement, ParentElement as _, Pixels, Styled as _, Window, div, px};

use super::Sides;
use crate::workspace_pane::HEADER_HEIGHT;

/// How far the window's buttons are asked to sit from its top and its left.
pub(crate) const WINDOW_BUTTONS_INSET: Pixels = px(12.);
/// How wide the window's three buttons are together. macOS draws them, and on macOS 27 each is 14px wide and they're
/// 9px apart.
const WINDOW_BUTTONS_WIDTH: Pixels = px(60.);
/// The space between the window's buttons and the button beside them, which leaves that button's icon about as far
/// from them as they are from each other.
const AFTER_WINDOW_BUTTONS: Pixels = px(6.);
/// The button that hides and shows the sidebar on the left: the kit's small button.
const TOGGLE_WIDTH: Pixels = px(24.);
/// How wide each of Windows' three buttons is, as Windows draws its own.
pub(crate) const WINDOWS_BUTTON_WIDTH: Pixels = px(46.);

/// How far what's in the strip keeps from each end of the window in the frame being drawn.
#[derive(Clone, Copy, Default)]
pub(super) struct Clearance {
    /// From the left, to clear the window's buttons and the button beside them.
    corner_inset: Pixels,
    /// How much of the strip's right end the window's buttons take.
    end_room: Pixels,
    /// From the right, to clear the window's buttons there.
    end_inset: Pixels,
}

impl Clearance {
    /// With `shown` of each sidebar, left then right, from the window's edge.
    pub(super) fn with_shown(window: &Window, shown: (Pixels, Pixels)) -> Self {
        // By as much of the window's buttons, and the button beside them, as the sidebar on the left doesn't cover,
        // which is none while it's open, since even at its narrowest it covers them.
        let corner_inset = (corner_starts(window) + TOGGLE_WIDTH - shown.0).max(px(0.));
        // The same at the right end, which the sidebar there covers whenever it's open.
        let end_room = end_room(window);
        Self { corner_inset, end_room, end_inset: (end_room - shown.1).max(px(0.)) }
    }
}

/// What reaches the strip's right end, which keeps clear of the window's buttons there in its own way.
#[derive(Clone, Copy)]
pub(crate) enum AtTheTop {
    /// The workspace's header, or Home's bar, which the sidebar on the right covers the buttons of while it's open.
    Header,
    /// The top of the sidebar on the right, under the buttons whenever it's open.
    RightSidebar,
    /// What sits above the header and fills the strip, as an error does.
    AboveTheHeader,
}

/// Where the button that hides and shows the sidebar on the left starts, from the window's left edge: to the right of
/// the window's buttons, where the window has them in that corner, which is on a Mac. In full screen macOS puts
/// them away, so it starts where they did.
fn corner_starts(window: &Window) -> Pixels {
    if cfg!(target_os = "macos") && !window.is_fullscreen() {
        WINDOW_BUTTONS_INSET + WINDOW_BUTTONS_WIDTH + AFTER_WINDOW_BUTTONS
    } else {
        WINDOW_BUTTONS_INSET
    }
}

/// How much of the strip's right end the window's buttons take: on Windows, where the app draws them, except in full
/// screen, which has none. On a Mac they're in the top left corner.
pub(crate) fn end_room(window: &Window) -> Pixels {
    if cfg!(target_os = "windows") && !window.is_fullscreen() { WINDOWS_BUTTON_WIDTH * 3. } else { px(0.) }
}

/// How far what's at the top of the window starts from where it otherwise would, to clear the window's buttons and
/// the button beside them while the sidebar on the left doesn't cover them.
pub(crate) fn corner_inset(cx: &App) -> Pixels {
    Sides::of(cx).placed.clearance.corner_inset
}

/// The same for the workspace's header and Home's bar, which are clear of the window's buttons already when
/// something sits above them.
pub(crate) fn header_inset(cx: &App) -> Pixels {
    let placed = Sides::of(cx).placed;
    if placed.beside.above_the_header { px(0.) } else { placed.clearance.corner_inset }
}

/// How far `what` ends from where it otherwise would, to clear the window's buttons at the strip's right end. The
/// header and the sidebar's top are clear of them already when something sits above them.
pub(crate) fn end_inset(what: AtTheTop, cx: &App) -> Pixels {
    let placed = Sides::of(cx).placed;
    let below = placed.beside.above_the_header;
    match what {
        AtTheTop::Header if !below => placed.clearance.end_inset,
        AtTheTop::RightSidebar if !below => placed.clearance.end_room,
        AtTheTop::AboveTheHeader => placed.clearance.end_room,
        AtTheTop::Header | AtTheTop::RightSidebar => px(0.),
    }
}

/// The strip in the window's top left corner that holds `toggle`, the button that hides and shows the sidebar on the
/// left. It's over the sidebar while that shows and over the header while it doesn't, so it never moves.
pub(crate) fn corner(toggle: impl IntoElement, window: &Window) -> impl IntoElement {
    div().absolute().top_0().left(corner_starts(window)).h(HEADER_HEIGHT).flex().items_center().child(toggle)
}
