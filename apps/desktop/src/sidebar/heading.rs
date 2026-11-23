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
/// Add project: on this computer's hub, a menu of a folder here or a repository on GitHub, and on another hub, the
/// dialog of what its GitHub accounts can see.
fn add_button(app: &AppView, can_add: bool, cx: &mut Context<AppView>) -> AnyElement {
    let button = look::button("add-project", look::ButtonKind::Faint, look::ControlSize::Small, "Add project")
        .icon(IconName::Plus)
        .off(!can_add);
    if app.hub.remote.is_some() {
        return button.on_click(cx.listener(|t, _, window, cx| t.add_from_github(window, cx))).into_any_element();
    }
    let (from_github, from_folder) = (cx.weak_entity(), cx.weak_entity());
    button
        .menu(Anchor::TopRight, move |menu: look::Menu, _: &mut Window, _| {
            let (github, folder) = (from_github.clone(), from_folder.clone());
            menu.row(look::menu_row("From GitHub…").on_choose(move |window, cx| {
                let _ = github.update(cx, |t, cx| t.add_from_github(window, cx));
            }))
            .row(look::menu_row("From a Folder…").on_choose(move |_, cx| {
                let _ = folder.update(cx, AppView::add_folder);
            }))
        })
        .into_any_element()
}

/// Before it, the button that chooses the order, while the pointer is on the heading.
pub(super) fn heading(app: &AppView, connected: bool, cx: &mut Context<AppView>) -> AnyElement {
    let faint = look::colors(cx).text_muted.opacity(0.7);
    let can_add = connected;
    h_flex()
        .id(GROUP)
        .group(GROUP)
        .w_full()
        .h(HEADING_SIZE.height())
        .px(ROW_PAD)
        .child(div().flex_1().typeset(look::Type::Detail).emphasis().text_color(faint).child("Workspaces"))
        .child(sort_button(app, cx))
        .child(add_button(app, can_add, cx))
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
