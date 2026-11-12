//! The heading over the projects: what the list is, the button that chooses its order, and Add project.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement, Styled,
    TestSupportExt as _, Window, div,
};
use look::{Hand as _, Off as _};

use super::{HEADING_HEIGHT, SORTS};
use crate::app::AppView;

/// The heading, whose hover shows the button that chooses the order.
const GROUP: &str = "workspaces-heading";

/// What the list is, small and faint, as a section's label, and at its right end the Add project button, in words, so
/// it isn't taken for a repository's button for a new workspace under it. Greyed out while there's no hub to add to.
/// Before it, the button that chooses the order, while the pointer is on the heading.
pub(super) fn heading(app: &AppView, connected: bool, cx: &mut Context<AppView>) -> AnyElement {
    let faint = cx.theme().muted_foreground.opacity(0.7);
    h_flex()
        .id(GROUP)
        .group(GROUP)
        .w_full()
        .h(HEADING_HEIGHT)
        .px_1p5()
        .child(div().flex_1().text_xs().font_weight(FontWeight::MEDIUM).text_color(faint).child("Workspaces"))
        .child(sort_button(app, faint, cx))
        .child(
            Button::new("add-project")
                .ghost()
                .xsmall()
                .icon(IconName::Plus)
                .label("Add project")
                .text_color(faint)
                .off_while(!connected)
                .on_click(cx.listener(|t, _, _, cx| t.add_repository(cx))),
        )
        .test_support()
        .into_any_element()
}

/// The button whose menu chooses the order, with the one chosen ticked. It shows only while the pointer is on the
/// heading, or its menu is open, so the list stays quiet, as a row's archive button does.
fn sort_button(app: &AppView, faint: Hsla, cx: &mut Context<AppView>) -> impl IntoElement {
    let current = crate::settings::get(cx).sidebar_sort;
    let opens = cx.weak_entity();
    Button::new("sort-workspaces")
        .hand()
        .ghost()
        .xsmall()
        .icon(Lucide::ArrowUpDown)
        .text_color(faint)
        .tooltip("Sort workspaces")
        .when(!app.sidebar.sort_menu_open, |button| button.invisible().group_hover(GROUP, Styled::visible))
        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu: PopupMenu, _: &mut Window, _| {
            // A setting, which lasts once the app quits, and which the app draws again for when it changes.
            SORTS.into_iter().fold(menu.label("Sort by"), |menu, sort| {
                let item = PopupMenuItem::new(sort.label()).checked(sort == current).on_click(move |_, _, cx| {
                    crate::settings::change(cx, |settings| settings.sidebar_sort = sort);
                });
                menu.item(item)
            })
        })
        .on_open_change(move |open, _, cx| {
            let _ = opens.update(cx, |t, cx| {
                t.sidebar.sort_menu_open = *open;
                cx.notify();
            });
        })
}
