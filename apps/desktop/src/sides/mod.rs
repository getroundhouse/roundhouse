//! What the window's two sidebars share: each shows or hides with a slide, and is as wide as its inner edge was last
//! dragged to. The one on the left lists the workspaces, and the one on the right the changes. What's chosen for each
//! holds in every workspace until the app quits.
//!
//! What's beside a sidebar moves with it as it slides, so it's laid out again on every frame of the slide, and only
//! then. How far each has slid is kept here rather than by a view, because two views lay out by the one on the left:
//! the window, which draws it, and the workspace, whose header starts clear of the window's buttons. The window
//! works out where both are once, as it starts to draw a frame, and every view reads that, so no two of them
//! disagree by the moment between their asking.

mod clearance;

pub(crate) use clearance::{
    AtTheTop, MenusButton, WINDOW_BUTTONS_INSET, WINDOWS_BUTTONS_WIDTH, WindowButtons, clearance, corner_starts,
    placed_buttons, toggle_starts,
};

#[cfg(test)]
mod tests;

mod placement;

#[cfg(test)]
pub(crate) use placement::Chosen;
use placement::Sides;
pub(crate) use placement::{Beside, Side, Sidebar, frame, place};
#[cfg(test)]
use placement::{Change, Placed, dragged};
