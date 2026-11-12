//! A workspace's row in the sidebar: its mark and name, the button that archives it under the pointer, what it's
//! waiting on the hub for, and what a right-click on it offers.

use std::rc::Rc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, AnyElement, App, Context, ElementId, FontWeight, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement, Role, StatefulInteractiveElement as _, Styled, TestSupportExt as _, Window, div,
};
use look::{Hand as _, LineText, Off as _, Placed};
use protocol::model::Workspace;

use super::{MARK_GAP, ROW_HEIGHT, ROW_INSET, Standing, standing};
use crate::app::AppView;
use crate::right_click;
use crate::{ArchiveWorkspace, MarkWorkspace};

pub(super) fn workspace_row(
    app: &AppView,
    workspace: &Workspace,
    line_text: &LineText,
    cx: &mut Context<AppView>,
) -> AnyElement {
    // While Home shows, it's what's showing, rather than the workspace still selected under it.
    let selected = !app.home_showing() && app.selected.as_ref() == Some(&workspace.id);
    let (id, pointed) = (workspace.id.clone(), workspace.id.clone());
    let muted = cx.theme().muted_foreground;
    let Standing { signal: mark, work, needs_a_look } = standing(app, workspace, cx);
    let menu = row_menu(app, workspace, cx);
    let label = workspace.label().to_string();
    let pointing = app.pointing_at.as_ref() == Some(&workspace.id);
    let retitling = app.retitling.contains(&workspace.id).then_some("Renaming…");
    let waiting = app.busy.doing(&workspace.id).or(retitling);
    let row = Button::new(ElementId::Name(format!("workspace-{}", workspace.id).into()))
        .hand()
        .accessibility_label(label.clone())
        .ghost()
        .w_full()
        .h(ROW_HEIGHT)
        .pl(ROW_INSET)
        .pr_1p5()
        .selected(selected)
        .on_hover(cx.listener(move |t, hovered: &bool, _, cx| {
            // Leaving one row and coming onto the next can be heard in either order.
            if *hovered {
                t.pointing_at = Some(pointed.clone());
            } else if t.pointing_at.as_ref() == Some(&pointed) {
                t.pointing_at = None;
            } else {
                return;
            }
            cx.notify();
        }))
        .on_click(cx.listener(move |t, _, _, cx| t.open_workspace(id.clone(), cx)))
        .child(
            h_flex()
                .w_full()
                .gap(MARK_GAP)
                .text_sm()
                .when(needs_a_look, |d| d.font_weight(FontWeight::MEDIUM))
                // Rows with nothing to look at recede, so those with a reply or a question stand out. Selecting a row
                // only changes its background: font smoothing thickens brighter text, so a name that brightened would
                // look heavier too.
                .when(!needs_a_look, |d| d.text_color(muted))
                .child(look::mark(mark, work, look::Size::Row, cx))
                .child(
                    line_text
                        .body
                        .set(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
                        .child(label),
                )
                .when_some(waiting, |d, waiting| {
                    d.child(doing(format!("doing-{}", workspace.id), waiting, &line_text.small))
                })
                .when(waiting.is_none() && pointing, |d| d.child(archive_button(app, workspace, cx))),
        );
    row.context_menu(menu).into_any_element()
}

/// What a workspace is waiting on the hub for, such as `Archiving…`, in place of its button to do that. It shows
/// whether a button, a menu or a key started it, and wherever the pointer is, so it's clear the press took. It's
/// smaller than the workspace's name, so it's set on the name's baseline.
pub(crate) fn doing(id: String, waiting: &'static str, small: &Placed) -> impl IntoElement {
    let text = small.set(div().flex_none().text_color(look::faint()).child(waiting));
    div().id(ElementId::Name(id.into())).role(Role::Status).aria_label(waiting).child(text).test_support()
}

/// Archives the workspace without opening it. Drawn only while the pointer is over the row, so no name is cut short
/// to keep room for it. The size and colour of its repository's button for a new workspace, which the row's padding
/// lines it up under.
fn archive_button(app: &AppView, workspace: &Workspace, cx: &mut Context<AppView>) -> impl IntoElement {
    let id = workspace.id.clone();
    let button = Button::new(ElementId::Name(format!("archive-{}", workspace.id).into()))
        .ghost()
        .small()
        .icon(Lucide::Archive)
        .text_color(cx.theme().muted_foreground)
        .tooltip("Archive")
        // So it can't be pressed twice. A disabled button still takes the click, so the row doesn't open instead.
        .off_while(!app.busy.is_free())
        // A left press stops here, so the row under it isn't pressed. A pressed row would open the workspace that's
        // going once let go. It would also say the pointer had left at the first move, which a quick press makes, and
        // stop drawing this button before the press was let go. A right press goes on to the row's menu.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |t, _, _, cx| t.archive(id.clone(), false, cx)));
    div().flex_none().child(button)
}

/// What a right-click on a workspace's row offers: what a tab's does, a new title from what's been asked in it, then
/// archiving it. The keys act on the workspace showing, so only its row's menu shows them. Marking its chats needs one
/// with a reply. A new title needs one to replace, which the first prompt gives, none already on its way, and a
/// workspace that isn't on its way out. The sidebar lists no archived workspace, so every row can be archived.
fn row_menu(
    app: &AppView,
    workspace: &Workspace,
    cx: &mut Context<AppView>,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    let marking = app.workspace_marking(&workspace.id, cx);
    let (this, id) = (cx.weak_entity(), workspace.id.clone());
    let to_mark = this.clone();
    let marking = marking.map(|(marking, chats)| {
        let mark = move |cx: &mut App| {
            let _ = to_mark.update(cx, |t, cx| t.mark_chats(marking, chats.clone(), cx));
        };
        (marking, mark)
    });
    let to_archive = this.clone();
    // Not while Home hides it, when its keys do nothing.
    let showing = !app.home_showing() && app.selected.as_ref() == Some(&workspace.id);
    let marking_key = showing.then(|| Box::new(MarkWorkspace) as Box<dyn Action>);
    let shared = right_click::menu(marking, marking_key, move |window, cx| {
        let _ = this.update(cx, |t, cx| t.start_renaming(id.clone(), window, cx));
    });
    let (busy, id) = (!app.busy.is_free(), workspace.id.clone());
    let (to_retitle, retitled, holds) = (to_archive.clone(), workspace.id.clone(), to_archive.clone());
    let going = app.busy.doing(&workspace.id).is_some();
    let cant_retitle = going || workspace.title.is_none() || app.retitling.contains(&workspace.id);
    let retitle = Rc::new(move |cx: &mut App| {
        let _ = to_retitle.update(cx, |t, cx| t.regenerate_title(retitled.clone(), cx));
    });
    let archive = Rc::new(move |cx: &mut App| {
        let _ = to_archive.update(cx, |t, cx| t.archive(id.clone(), false, cx));
    });
    move |menu, window, cx| {
        // Built as it opens, so this is when the order starts to hold still for it.
        let opened = cx.entity();
        let _ = holds.update(cx, |t, cx| crate::reorder::hold_while_open(t, &opened, |t| &mut t.sidebar.hold, cx));
        let (retitle, archive) = (retitle.clone(), archive.clone());
        let regenerate = PopupMenuItem::new("Regenerate title").icon(Lucide::PencilSparkles);
        let menu =
            shared(menu, window, cx).item(regenerate.disabled(cant_retitle).on_click(move |_, _, cx| retitle(cx)));
        let item = PopupMenuItem::new("Archive").icon(Lucide::Archive);
        let item = item.when(showing, |item| item.action(Box::new(ArchiveWorkspace)));
        menu.separator().item(item.disabled(busy).on_click(move |_, _, cx| archive(cx)))
    }
}
