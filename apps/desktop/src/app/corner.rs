//! What the window does for its two sidebars: says where they are in each frame, and draws the button in its top left
//! corner that hides and shows the one on the left.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, Sizable as _};
use gpui_kit::{Context, Window};
use look::Hand as _;

use super::AppView;
use crate::sides::{self, Beside, Side, Sidebar};

impl AppView {
    /// Works out where the sidebars are for the frame the window is starting to draw, which every view in it goes
    /// by: around a workspace's changes only where a workspace shows, and with the header clear of the window's
    /// buttons already while an error sits above it.
    pub(super) fn place_sidebars(&self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.pane_to_draw().filter(|_| !self.settings_open() && !self.home_showing());
        let changes = pane.is_some_and(|pane| pane.read(cx).lists_changes());
        sides::place(Beside { changes, above_the_header: self.error.is_some() }, window, cx);
    }
}

/// Hides and shows the sidebar, as the button at the other end of the workspace's header does the sidebar of
/// changes. It's beside the window's buttons whether the sidebar shows or not, so it's always in the same place.
pub(super) fn sidebar_toggle(cx: &mut Context<AppView>) -> Button {
    Button::new("toggle-sidebar")
        .hand()
        .ghost()
        .small()
        .icon(IconName::PanelLeft)
        .tooltip(match Side::Left.chosen(cx).sidebar {
            Sidebar::Open => "Hide sidebar",
            Sidebar::Hidden => "Show sidebar",
        })
        .on_click(cx.listener(|_, _, _, cx| {
            // While the sidebar is hidden the button is over whatever is in the strip, such as an error, which would
            // take the click too.
            cx.stop_propagation();
            Side::Left.toggle(cx);
            cx.notify();
        }))
}
