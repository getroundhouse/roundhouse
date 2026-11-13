//! Home's bar, along the window's top: the search, what the list shows and in what order, and Create.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{Icon, IconName, h_flex};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement, Styled, TestSupportExt as _, WeakEntity, Window, div,
    px,
};
use look::{Hand as _, Off as _, Sizing as _};

use super::{ArchiveFilter, SORTS};
use crate::app::AppView;
use crate::sides::{self, AtTheTop};
use crate::workspace_pane::HEADER_HEIGHT;

/// In the strip, clear of the window's buttons at both ends as the workspace's header is.
pub(super) fn bar(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let colors = look::colors(cx);
    let (border, muted) = (colors.edge, colors.text_muted);
    let clear = sides::clearance(AtTheTop::Header, cx);
    h_flex()
        .flex_none()
        .w_full()
        .h(HEADER_HEIGHT)
        .pl(px(16.) + clear.start)
        .pr(px(8.) + clear.end)
        .gap_1()
        .border_b_1()
        .border_color(border)
        .child(
            // In a box of its own, so a test can see where the search starts. It hides the strip it's in from the
            // pointer, so on Windows a press here is an ordinary one in the window rather than one on a title bar,
            // and the search hears where it goes as it selects.
            div()
                .id("home-search")
                .flex_1()
                .min_w_0()
                .occlude()
                .child(
                    Input::new(&app.home.search)
                        .appearance(false)
                        .prefix(Icon::new(IconName::Search).text_color(muted)),
                )
                .test_support(),
        )
        .child(repos(app, cx))
        .child(archived(app, cx))
        .child(sort(app, cx))
        .child(
            Button::new("home-create")
                .primary()
                .sized(look::ControlSize::Regular)
                .icon(IconName::Plus)
                .label("Create")
                // In the repository the list shows, as `current_repo` says. The dialog opens while another workspace
                // is being made, and its Create waits.
                .off_while(app.current_repo().is_none())
                .on_click(cx.listener(|t, _, window, cx| t.new_workspace_here(window, cx))),
        )
}

/// A filter: borderless, muted, with what it's set to and a menu to change it.
fn filter(id: &'static str, icon: impl Into<Icon>, label: String, cx: &Context<AppView>) -> Button {
    Button::new(id)
        .hand()
        .ghost()
        .sized(look::ControlSize::Regular)
        .icon(icon)
        .label(label)
        .dropdown_caret(true)
        .text_color(look::colors(cx).text_muted)
}

fn repos(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let current = app.home_repo().cloned();
    let name = current.as_ref().and_then(|id| app.repo(id)).map(|r| r.name.clone());
    let label = name.map_or_else(|| "All projects".to_string(), |name| format!("In {name}"));
    let choices: Vec<_> = app.records().repos().iter().map(|r| (r.id.clone(), r.name.clone())).collect();
    let this = cx.weak_entity();
    filter("home-repos", IconName::Folder, label, cx).dropdown_menu(move |menu: PopupMenu, _: &mut Window, _| {
        let all = menu_item(&this, "All projects", current.is_none(), |t, cx| t.filter_home(None, cx));
        choices.iter().fold(menu.item(all).separator(), |menu, (id, name)| {
            let id = id.clone();
            let checked = current.as_ref() == Some(&id);
            menu.item(menu_item(&this, name, checked, move |t, cx| t.filter_home(Some(id.clone()), cx)))
        })
    })
}

fn archived(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let current = app.home.archived;
    let this = cx.weak_entity();
    filter("home-archived", Lucide::Archive, current.label().to_string(), cx).dropdown_menu(
        move |menu: PopupMenu, _: &mut Window, _| {
            ArchiveFilter::ALL.into_iter().fold(menu, |menu, archived| {
                let pick = move |t: &mut AppView, cx: &mut Context<AppView>| t.filter_archived_in_home(archived, cx);
                menu.item(menu_item(&this, archived.label(), archived == current, pick))
            })
        },
    )
}

fn sort(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let current = app.home.sort;
    let this = cx.weak_entity();
    filter("home-sort", Lucide::ArrowUpDown, current.label().to_string(), cx).dropdown_menu(
        move |menu: PopupMenu, _: &mut Window, _| {
            SORTS.into_iter().fold(menu, |menu, sort| {
                let pick = move |t: &mut AppView, cx: &mut Context<AppView>| t.sort_home(sort, cx);
                menu.item(menu_item(&this, sort.label(), sort == current, pick))
            })
        },
    )
}

/// One choice in a filter's menu, ticked when it's the one set.
fn menu_item(
    this: &WeakEntity<AppView>,
    label: &str,
    checked: bool,
    pick: impl Fn(&mut AppView, &mut Context<AppView>) + 'static,
) -> PopupMenuItem {
    let this = this.clone();
    PopupMenuItem::new(label.to_string()).checked(checked).on_click(move |_, _, cx| {
        let _ = this.update(cx, |t, cx| pick(t, cx));
    })
}
