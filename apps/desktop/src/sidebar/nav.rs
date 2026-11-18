//! The top of the sidebar, above the workspaces: Home, a new workspace, and search.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{IconName, IconNamed, h_flex, v_flex};
use gpui_kit::{Context, IntoElement, ParentElement, Styled};

use super::{LIST_PAD, ROW_SIZE, row_icon};
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
        .gap(look::Space::XSmall)
        .border_b_1()
        .border_color(border)
        .child(super::hub::render(app, cx))
        .child(
            item("home", Lucide::House, "Home", cx)
                .chosen(app.home_showing())
                .on_click(cx.listener(|t, _, window, cx| t.open_home(window, cx))),
        )
        .child(
            item("create", IconName::Plus, "Create", cx)
                .off(!can_create)
                .on_click(cx.listener(|t, _, window, cx| t.new_workspace_here(window, cx))),
        )
        // Search isn't built, so it's only a place for it.
        .child(item("search", IconName::Search, "Search", cx))
}

/// An item: a muted icon and label, chosen while what it opens is showing.
fn item(id: &'static str, icon: impl IconNamed, label: &'static str, cx: &mut Context<AppView>) -> look::ListRow {
    let muted = look::colors(cx).text_muted;
    let content = h_flex().w_full().gap(look::Space::Small).text_color(muted).child(row_icon(icon, muted)).child(label);
    look::list_row(id, ROW_SIZE).button(label).child(content)
}
