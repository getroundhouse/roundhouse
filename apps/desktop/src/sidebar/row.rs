//! A workspace's row in the sidebar: its mark and name, a pencil while something typed in it isn't sent, a clock while
//! one of its chats' caches is about to expire, the button that archives it under the pointer, which they make room
//! for, what it's waiting on the hub for, and what a right-click on it offers.

use std::rc::Rc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::base::motion::transition;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, AnyElement, App, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement, Pixels, Role,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, Window, div, px,
};
use look::{IconSizing as _, LineText, Placed};
use protocol::model::{Workspace, WorkspaceId};

use super::{MARK_GAP, MARK_INDENT, ROW_SIZE, Standing, standing};
use crate::app::AppView;
use crate::{ArchiveWorkspace, MarkWorkspace};
use crate::{prompt_cache, right_click};

/// As wide as the button that archives a row, a small button's square.
const BUTTON: Pixels = look::ControlSize::Small.height();
/// How wide the box each icon at a row's end is centred in: as big as a place's or a project's icon at its row's start,
/// so the pencil, which is smaller, and the cache's clock sit as the sidebar's icons do.
const ICON_BOX: Pixels = look::IconSize::Large.size();

pub(super) fn workspace_row(
    app: &AppView,
    workspace: &Workspace,
    line_text: &LineText,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> AnyElement {
    // While Home shows, it's what's showing, rather than the workspace still selected under it.
    let selected = !app.home_showing() && app.selected.as_ref() == Some(&workspace.id);
    let id = workspace.id.clone();
    let muted = look::colors(cx).text_muted;
    let Standing { signal: mark, work, needs_a_look } = standing(app, workspace, cx);
    let menu = row_menu(app, workspace, cx);
    let label = workspace.label().to_string();
    let pointing = app.pointing_at.as_ref() == Some(&workspace.id);
    let retitling = app.retitling.contains(&workspace.id).then_some("Renaming…");
    let waiting = crate::busy::doing(workspace.status).or(retitling);
    // Not on the workspace showing, whose composer is in view, but while Home shows in its place. A workspace not shown
    // since the app opened has only what the hub keeps.
    let drafted = !selected
        && match app.panes.get(&workspace.id) {
            Some(pane) => pane.read(cx).has_unsent(cx),
            None => crate::workspace_pane::drafted(app.records().sessions(&workspace.id)),
        };
    let its_chats = app.records().sessions(&workspace.id).iter();
    let expiring = prompt_cache::soonest_expiring(its_chats, look::now_ms(cx));
    let end = End { drafted, expiring, pointing, waiting };
    let end = right_end(app, workspace, end, &line_text.detail, window, cx);
    let row = look::list_row(ElementId::Name(format!("workspace-{}", workspace.id).into()), ROW_SIZE);
    let row = follows_the_pointer(row, &workspace.id, cx)
        .button(label.clone())
        .indent(MARK_INDENT.size())
        .chosen(selected)
        .on_click(cx.listener(move |t, _, _, cx| t.open_workspace(id.clone(), cx)))
        .child(
            h_flex()
                .w_full()
                .gap(MARK_GAP)
                .when(needs_a_look, look::Typeset::emphasis)
                // Rows with nothing to look at recede, so those with a reply or a question stand out. Selecting a row
                // only changes its background: font smoothing thickens brighter text, so a name that brightened would
                // look heavier too.
                .when(!needs_a_look, |d| d.text_color(muted))
                .child(look::mark(mark, work, look::Size::Row, cx))
                // The right end keeps the room between the name and it, so that room can come and go with it.
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            line_text
                                .body
                                .set(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
                                .child(label),
                        )
                        .children(end),
                ),
        );
    row.right_click_menu(menu).into_any_element()
}

/// Has the row say when the pointer is over it, which is when it shows its archive button.
fn follows_the_pointer(row: look::ListRow, id: &WorkspaceId, cx: &mut Context<AppView>) -> look::ListRow {
    let (pointed, pressed, let_go) = (id.clone(), id.clone(), id.clone());
    row.on_hover(cx.listener(move |t, hovered: &bool, _, cx| {
        // Leaving one row and coming onto the next can be heard in either order.
        if *hovered {
            t.pointing_at = Some(pointed.clone());
        } else if t.pointing_at.as_ref() == Some(&pointed) && t.sidebar.pressed.as_ref() != Some(&pointed) {
            t.pointing_at = None;
        } else {
            return;
        }
        cx.notify();
    }))
    // A pressed row says the pointer has left at the first move, which the listener above then leaves alone, so
    // the button and what made room for it don't go and come back around a click. Let go over the row, GPUI says
    // again where the pointer is. Let go anywhere else, nothing does, so the pointer is taken to have left.
    .on_press(cx.listener(move |t, _, _, _| t.sidebar.pressed = Some(pressed.clone())))
    .on_let_go(cx.listener(|t, _, _, _| t.sidebar.pressed = None))
    .on_let_go_outside(cx.listener(move |t, _, _, cx| {
        if t.sidebar.pressed.as_ref() != Some(&let_go) {
            return;
        }
        t.sidebar.pressed = None;
        if t.pointing_at.as_ref() == Some(&let_go) {
            t.pointing_at = None;
            cx.notify();
        }
    }))
}

/// What a row has at its right end.
#[derive(Clone, Copy)]
struct End {
    /// Something typed or attached in one of its chats isn't sent.
    drafted: bool,
    /// How long the soonest to expire of its chats' caches has left, of those about to.
    expiring: Option<i64>,
    /// The pointer is over the row.
    pointing: bool,
    /// What it's waiting on the hub for, if anything.
    waiting: Option<&'static str>,
}

/// A pencil while something typed in the workspace isn't sent, then a clock while one of its chats' caches is about to
/// expire, and after them what the workspace is waiting on the hub for, or else under the pointer the button that
/// archives it. The button's place is the row's right end, and whichever of the pencil and the clock is last rests in
/// the middle of it, under the project's button for a new workspace, so no name is cut short for a button that isn't
/// there. As the pointer comes, they slide aside while the button fades in, and back as it goes. Nothing says the
/// cache while the workspace is waiting on the hub, since that's it going or coming. None when there's nothing, so the
/// name has the row's width.
fn right_end(
    app: &AppView,
    workspace: &Workspace,
    end: End,
    small: &Placed,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> Option<AnyElement> {
    let muted = look::colors(cx).text_muted;
    let End { drafted, expiring, pointing, waiting } = end;
    let expiring = expiring.filter(|_| waiting.is_none());
    let icons = drafted || expiring.is_some();
    // How far the button has come, from 0 to 1. The kit keeps it for as long as the row is drawn, so it goes on from
    // wherever it has got to. A row scrolled out of sight partway finds it at its end once it's back, rather than
    // where it was, which only cuts short a fade that set off under the pointer and that nobody saw the rest of.
    let wanted = if pointing && waiting.is_none() { 1. } else { 0. };
    let came = ElementId::Name(format!("archive-came-{}", workspace.id).into());
    let came = transition(came, wanted, look::motion::fade(), window, cx);
    let button = waiting.is_none() && (pointing || came > 0.);
    if !icons && !button && waiting.is_none() {
        return None;
    }
    let pencil = Icon::new(Lucide::Pencil).sized(look::IconSize::Regular).text_color(muted);
    let pencil = icon_box(pencil)
        .id(ElementId::Name(format!("draft-{}", workspace.id).into()))
        .role(Role::Image)
        .aria_label("Draft")
        .test_support();
    let clock =
        expiring.map(|left_ms| icon_box(prompt_cache::clock(format!("row-cache-{}", workspace.id), left_ms, muted)));
    let icons_drawn = h_flex().gap(look::Space::XSmall).when(drafted, |d| d.child(pencil)).children(clock);
    // With nothing else at the row's end, the room between the name and the button comes and goes with the button.
    let before = if icons || waiting.is_some() { MARK_GAP.size() } else { MARK_GAP.size() * came };
    let end = h_flex().flex_none().pl(before).child(icons_drawn);
    // Text has no padding of its own to keep it off the pencil, as the button has.
    let end = if let Some(waiting) = waiting {
        let doing = doing(format!("doing-{}", workspace.id), waiting, small, cx);
        end.child(div().when(drafted, |d| d.pl(MARK_GAP)).child(doing))
    } else {
        // The icons rest in the middle of the button's place, half of what the button is wider than their box by in
        // from the row's right end, and end against its box once it has come. Its padding is then the room between
        // them and it: a little more than if they were set edge to edge, so they don't run together.
        let rests = if icons { (BUTTON - ICON_BOX) / 2. } else { px(0.) };
        let room = div().flex_none().h(BUTTON).w(rests + (BUTTON - rests) * came);
        // The button is whole and in its place from the first frame, over the room that's still opening for it, so a
        // press where it's coming lands on it rather than on the row.
        let button =
            button.then(|| div().absolute().top_0().right_0().opacity(came).child(archive_button(app, workspace, cx)));
        end.relative().child(room).children(button)
    };
    Some(end.into_any_element())
}

/// `icon` in the middle of a box as wide as the sidebar's icons.
fn icon_box(icon: impl IntoElement) -> gpui_kit::Div {
    div().flex_none().size(ICON_BOX).flex().items_center().justify_center().child(icon)
}

/// What a workspace is waiting on the hub for, such as `Archiving…`, in place of its button to do that. It shows
/// whether a button, a menu or a key started it, and wherever the pointer is, so it's clear the press took. It's
/// smaller than the workspace's name, so it's set on the name's baseline.
pub(crate) fn doing(id: String, waiting: &'static str, small: &Placed, cx: &App) -> impl IntoElement {
    let text = small.set(div().flex_none().text_color(look::colors(cx).text_faint).child(waiting));
    div().id(ElementId::Name(id.into())).role(Role::Status).aria_label(waiting).child(text).test_support()
}

/// Archives the workspace without opening it. Drawn only while the pointer is over the row, and as it fades once the
/// pointer has gone, so no name is cut short to keep room for it. The size and colour of its repository's button for
/// a new workspace, which the row's padding lines it up under.
fn archive_button(app: &AppView, workspace: &Workspace, cx: &mut Context<AppView>) -> impl IntoElement {
    let id = workspace.id.clone();
    let button = look::icon_button(
        ElementId::Name(format!("archive-{}", workspace.id).into()),
        look::ButtonKind::Quiet,
        look::ControlSize::Small,
        Lucide::Archive,
    )
    .tooltip("Archive")
    // So it can't be pressed twice. A disabled button still takes the click, so the row doesn't open instead.
    .off(!app.busy.is_free())
    // A left press stops here, so the row under it isn't pressed. A pressed row would open the workspace that's
    // going once let go. It would also say the pointer had left at the first move, which a quick press makes, and
    // start this button fading before the press was let go. A right press goes on to the row's menu.
    .keeps_its_press()
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
) -> impl Fn(look::Menu, &mut Window, &mut App) -> look::Menu + 'static {
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
    let going = crate::busy::doing(workspace.status).is_some();
    let cant_retitle = going || workspace.title.is_none() || app.retitling.contains(&workspace.id);
    let retitle = Rc::new(move |cx: &mut App| {
        let _ = to_retitle.update(cx, |t, cx| t.regenerate_title(retitled.clone(), cx));
    });
    let archive = Rc::new(move |cx: &mut App| {
        let _ = to_archive.update(cx, |t, cx| t.archive(id.clone(), false, cx));
    });
    move |menu, window, cx| {
        // Built as it opens, so this is when the order starts to hold still for it.
        let opened = menu.open();
        let _ = holds.update(cx, |t, cx| crate::reorder::hold_while_open(t, &opened, |t| &mut t.sidebar.hold, cx));
        let (retitle, archive) = (retitle.clone(), archive.clone());
        let regenerate = look::menu_row("Regenerate title").icon(Lucide::PencilSparkles);
        let menu = shared(menu, window, cx).row(regenerate.off(cant_retitle).on_choose(move |_, cx| retitle(cx)));
        let row = look::menu_row("Archive").icon(Lucide::Archive);
        let row = row.when(showing, |row| row.key(Box::new(ArchiveWorkspace)));
        menu.separator().row(row.off(busy || going).on_choose(move |_, cx| archive(cx)))
    }
}
