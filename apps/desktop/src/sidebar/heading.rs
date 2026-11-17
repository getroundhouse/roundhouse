//! The heading over the projects: what the list is, the button that chooses its order, and Add project.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{IconName, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement, Styled, TestSupportExt as _,
    Window, div,
};
use look::Typeset as _;

use super::{HEADING_SIZE, ROW_PAD, SORTS};
use crate::app::AppView;

/// The heading, whose hover shows the button that chooses the order.
const GROUP: &str = "workspaces-heading";

/// What the list is, small and faint, as a section's label, and at its right end the Add project button, in words, so
/// it isn't taken for a repository's button for a new workspace under it. Greyed out while there's no hub to add to.
/// Before it, the button that chooses the order, while the pointer is on the heading.
pub(super) fn heading(app: &AppView, connected: bool, cx: &mut Context<AppView>) -> AnyElement {
    let faint = look::colors(cx).text_muted.opacity(0.7);
    let remote = app.hub.remote.is_some();
    let can_add = connected && !remote;
    h_flex()
        .id(GROUP)
        .group(GROUP)
        .w_full()
        .h(HEADING_SIZE.height())
        .px(ROW_PAD)
        .child(div().flex_1().typeset(look::Type::Detail).emphasis().text_color(faint).child("Workspaces"))
        .child(sort_button(app, cx))
        .child(
            look::button("add-project", look::ButtonKind::Faint, look::ControlSize::Small, "Add project")
                .icon(IconName::Plus)
                .when(connected && remote, |b| b.tooltip("Add projects on that hub’s machine"))
                .off(!can_add)
                .on_click(cx.listener(|t, _, _, cx| t.add_repository(cx))),
        )
        .test_support()
        .into_any_element()
}

/// The button whose menu chooses the order, with the one chosen ticked. It shows only while the pointer is on the
/// heading, or its menu is open, so the list stays quiet, as a row's archive button does.
fn sort_button(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let current = crate::settings::get(cx).sidebar_sort;
    let opens = cx.weak_entity();
    look::icon_button("sort-workspaces", look::ButtonKind::Faint, look::ControlSize::Small, Lucide::ArrowUpDown)
        .tooltip("Sort workspaces")
        .when(!app.sidebar.sort_menu_open, |button| button.shown_while_over(GROUP))
        .menu(Anchor::TopRight, move |menu: look::Menu, _: &mut Window, _| {
            // A setting, which lasts once the app quits, and which the app draws again for when it changes.
            SORTS.into_iter().fold(menu.label("Sort by"), |menu, sort| {
                let row = look::menu_row(sort.label()).chosen(sort == current).on_choose(move |_, cx| {
                    crate::settings::change(cx, |settings| settings.sidebar_sort = sort);
                });
                menu.row(row)
            })
        })
        .on_open_change(move |open, _, cx| {
            let _ = opens.update(cx, |t, cx| {
                t.sidebar.sort_menu_open = *open;
                cx.notify();
            });
        })
}
