//! What the window does for its two sidebars: says where they are in each frame, and draws the buttons in its top left
//! corner: the one that hides and shows the sidebar on the left, and off the Mac the one before it that opens the
//! menus.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, Sizable as _};
use gpui_kit::{Context, Div, IntoElement, ParentElement as _, Pixels, Styled as _, Window, div};
use look::Hand as _;

use super::{AppView, menu_button, title_bar};
use crate::menus::System;
use crate::sides::{self, Beside, MenusButton, Side, Sidebar};
use crate::workspace_pane::HEADER_HEIGHT;

/// Whether the button that opens the menus is in the corner, which it is wherever the app draws them.
const MENUS: MenusButton = if System::THIS.draws_the_menus() { MenusButton::InTheCorner } else { MenusButton::Absent };

impl AppView {
    /// Works out where the sidebars are for the frame the window is starting to draw, which every view in it goes
    /// by: around a workspace's changes only where a workspace shows, with the header clear of the window's buttons
    /// already while an error sits above it, and around the window's buttons wherever the system has them.
    pub(super) fn place_sidebars(&self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.pane_to_draw().filter(|_| !self.settings_open() && !self.home_showing());
        let changes = pane.is_some_and(|pane| pane.read(cx).lists_changes());
        let buttons = title_bar::buttons_in_this_frame(window, cx);
        sides::place(Beside { changes, above_the_header: self.error.is_some() }, buttons, MENUS, window, cx);
    }
}

/// The place in the window's top left corner that holds the button that hides and shows the sidebar on the left. It's
/// over the sidebar while that shows and over the header while it doesn't, so it never moves.
pub(super) fn corner(cx: &mut Context<AppView>) -> impl IntoElement {
    in_the_corner(sides::toggle_starts(cx)).child(sidebar_toggle(cx))
}

/// The place before it that holds the button that opens the menus, wherever the app draws them. It's there over
/// Settings too, which has no sidebar to hide.
pub(super) fn menus(app: &AppView, cx: &mut Context<AppView>) -> Option<impl IntoElement> {
    match MENUS {
        MenusButton::Absent => None,
        MenusButton::InTheCorner => Some(in_the_corner(sides::corner_starts(cx)).child(menu_button::button(app, cx))),
    }
}

/// A place in the strip along the window's top, as tall as it, starting `starts` from the window's left edge.
fn in_the_corner(starts: Pixels) -> Div {
    div().absolute().top_0().left(starts).h(HEADER_HEIGHT).flex().items_center()
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
