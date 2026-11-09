//! The top of the sidebar, above the workspaces: Home, a new workspace, and search.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, IconName, IconNamed, Selectable as _, h_flex, v_flex};
use gpui_kit::{Context, IntoElement, ParentElement, Pixels, Styled, px};

use super::icon_box;
use crate::app::AppView;

/// Each item's height, as a row of body text with 6px above and below.
const ITEM_HEIGHT: Pixels = px(32.);

/// The block of items, with a line under it before the workspaces.
pub(super) fn nav(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let border = cx.theme().sidebar_border;
    let can_create = app.current_repo().is_some() && !app.busy;
    v_flex()
        .flex_none()
        .w_full()
        .p_1p5()
        .gap_0p5()
        .border_b_1()
        .border_color(border)
        .child(
            item("home", Lucide::House, "Home", cx)
                .selected(app.home_showing())
                .on_click(cx.listener(|t, _, window, cx| t.open_home(window, cx))),
        )
        .child(
            item("create", IconName::Plus, "Create", cx)
                .disabled(!can_create)
                .on_click(cx.listener(|t, _, _, cx| t.new_workspace_here(cx))),
        )
        // Search isn't built, so it's only a place for it.
        .child(item("search", IconName::Search, "Search", cx))
}

/// An item: a muted icon and label, filled while what it opens is showing.
fn item(id: &'static str, icon: impl IconNamed, label: &'static str, cx: &mut Context<AppView>) -> Button {
    let muted = cx.theme().muted_foreground;
    let content = h_flex().w_full().gap_2().text_sm().text_color(muted).child(icon_box(icon, muted)).child(label);
    Button::new(id).accessibility_label(label).ghost().w_full().h(ITEM_HEIGHT).px_1p5().child(content)
}
