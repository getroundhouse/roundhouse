//! What the window does for its two sidebars: says where they are in each frame, and draws the button in its top left
//! corner that hides and shows the one on the left.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, Sizable as _};
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _, Window, div};
use look::Hand as _;

use super::{AppView, title_bar};
use crate::sides::{self, Beside, Side, Sidebar};
use crate::workspace_pane::HEADER_HEIGHT;

impl AppView {
    /// Works out where the sidebars are for the frame the window is starting to draw, which every view in it goes
    /// by: around a workspace's changes only where a workspace shows, with the header clear of the window's buttons
    /// already while an error sits above it, and around the window's buttons wherever the system has them.
    pub(super) fn place_sidebars(&self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.pane_to_draw().filter(|_| !self.settings_open() && !self.home_showing());
        let changes = pane.is_some_and(|pane| pane.read(cx).lists_changes());
        let buttons = title_bar::window_buttons(window, cx);
        sides::place(Beside { changes, above_the_header: self.error.is_some() }, buttons, window, cx);
    }
}

/// The strip in the window's top left corner that holds the button that hides and shows the sidebar on the left. It's
/// over the sidebar while that shows and over the header while it doesn't, so it never moves.
pub(super) fn corner(cx: &mut Context<AppView>) -> impl IntoElement {
    let starts = sides::toggle_starts(cx);
    div().absolute().top_0().left(starts).h(HEADER_HEIGHT).flex().items_center().child(sidebar_toggle(cx))
}

/// Hides and shows the sidebar, as the button at the other end of the workspace's header does the sidebar of
/// changes. It stays put whether the sidebar shows or not, beside the window's buttons where they're in the corner.
fn sidebar_toggle(cx: &mut Context<AppView>) -> Button {
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
