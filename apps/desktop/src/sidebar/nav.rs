//! The top of the sidebar, above the workspaces: Home, a new workspace, and search.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, IconNamed, Selectable as _, h_flex, v_flex};
use gpui_kit::{Context, IntoElement, ParentElement, Styled};
use look::{Hand as _, Off as _, Typeset as _};

use super::{LIST_PAD, ROW_PAD, ROW_SIZE, icon_box};
use crate::app::AppView;

/// The block of items, with a line under it before the workspaces, and as much room around it as around the list, so
/// their icons line up.
pub(super) fn nav(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let border = look::colors(cx).edge;
    // The dialog opens while another workspace is being made, and its Create waits.
    let can_create = app.current_repo().is_some();
    v_flex()
        .flex_none()
        .w_full()
        .p(LIST_PAD)
        .gap_1()
        .border_b_1()
        .border_color(border)
        .child(
            item("home", Lucide::House, "Home", cx)
                .selected(app.home_showing())
                .hand()
                .on_click(cx.listener(|t, _, window, cx| t.open_home(window, cx))),
        )
        .child(
            item("create", IconName::Plus, "Create", cx)
                .off_while(!can_create)
                .on_click(cx.listener(|t, _, window, cx| t.new_workspace_here(window, cx))),
        )
        // Search isn't built, so it's only a place for it.
        .child(item("search", IconName::Search, "Search", cx))
}

/// An item: a muted icon and label, filled while what it opens is showing.
fn item(id: &'static str, icon: impl IconNamed, label: &'static str, cx: &mut Context<AppView>) -> Button {
    let muted = look::colors(cx).text_muted;
    let content =
        h_flex().w_full().gap_2().typeset(look::Type::Body).text_color(muted).child(icon_box(icon, muted)).child(label);
    Button::new(id).accessibility_label(label).ghost().w_full().h(ROW_SIZE.height()).px(ROW_PAD).child(content)
}
