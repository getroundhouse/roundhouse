//! Home: every workspace in one list, shown in place of the workspace, to find one by its name, narrow it to a
//! repository or to what's archived, and sort.

mod bar;

use std::rc::Rc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::v_virtual_list;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, VirtualListScrollHandle, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, ElementId, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Pixels, StatefulInteractiveElement as _, Styled, Subscription, TestSupportExt as _, Window, div, px,
    size,
};
use look::{Hand as _, InRegion as _, LineText, Off as _, Placed, Region};
use protocol::model::{RepoId, Workspace, WorkspaceStatus};

use crate::app::AppView;
use crate::sidebar::{Standing, doing, standing};
use crate::workspace_order::Sort;

const ROW_HEIGHT: Pixels = px(44.);
/// The repository's column, wide enough for most names.
const REPO_WIDTH: Pixels = px(160.);

/// What Home shows, which stays the same from one time it's open to the next.
pub(crate) struct Home {
    pub(crate) search: Entity<InputState>,
    /// Only this repository's workspaces, or every repository's.
    pub(crate) repo: Option<RepoId>,
    pub(crate) archived: ArchiveFilter,
    pub(crate) sort: Sort,
    /// Where the list is scrolled to. The list is made again each frame, so it can't keep that itself.
    scroll: VirtualListScrollHandle,
    /// Lists again as the search is typed.
    _following: Subscription,
}

impl Home {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let following = cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        Self {
            search,
            repo: None,
            archived: ArchiveFilter::Hide,
            sort: Sort::LastActive,
            scroll: VirtualListScrollHandle::new(),
            _following: following,
        }
    }
}

/// Which workspaces the list shows, by whether they're archived.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ArchiveFilter {
    Hide,
    Show,
    Only,
}

impl ArchiveFilter {
    pub(crate) const ALL: [ArchiveFilter; 3] = [ArchiveFilter::Hide, ArchiveFilter::Show, ArchiveFilter::Only];

    pub(crate) fn label(self) -> &'static str {
        match self {
            ArchiveFilter::Hide => "Hiding archived",
            ArchiveFilter::Show => "Showing all",
            ArchiveFilter::Only => "Only archived",
        }
    }

    /// A workspace still being made isn't listed, as the sidebar leaves it out, since there's nothing to show of it
    /// yet. Neither is one in a state this app doesn't know.
    fn lists(self, status: WorkspaceStatus) -> bool {
        use ArchiveFilter::{Hide, Only, Show};
        use WorkspaceStatus::{Archived, Creating, Ready, Unknown};
        match (self, status) {
            (Hide | Show, Ready) | (Show | Only, Archived) => true,
            (_, Creating | Unknown) | (Hide, Archived) | (Only, Ready) => false,
        }
    }
}

/// The orders the list can be in.
pub(crate) const SORTS: [Sort; 3] = [Sort::LastActive, Sort::Newest, Sort::Name];

/// The workspaces the list shows, by where they are in `workspaces`, in its order. The search matches the name or the
/// branch, in any case.
fn listed(
    workspaces: &[Workspace],
    repo: Option<&RepoId>,
    archived: ArchiveFilter,
    sort: Sort,
    search: &str,
) -> Vec<usize> {
    let search = search.trim().to_lowercase();
    let matches = |w: &Workspace| {
        search.is_empty() || w.label().to_lowercase().contains(&search) || w.branch.to_lowercase().contains(&search)
    };
    let mut listed: Vec<usize> = (0..workspaces.len())
        .filter(|&i| {
            let w = &workspaces[i];
            repo.is_none_or(|repo| &w.repo_id == repo) && archived.lists(w.status) && matches(w)
        })
        .collect();
    sort.sort(&mut listed, workspaces);
    listed
}

pub(crate) fn page(app: &AppView, inset: Pixels, cx: &mut Context<AppView>) -> impl IntoElement {
    let home = &app.home;
    let search = home.search.read(cx).value().to_string();
    let rows: Rc<[usize]> = listed(&app.workspaces, app.home_repo(), home.archived, home.sort, &search).into();
    let empty = rows.is_empty();
    let sizes = Rc::new(vec![size(px(0.), ROW_HEIGHT); rows.len()]);
    // A moving mark builds this view again on every frame, so only the rows on screen are built.
    let list = v_virtual_list(cx.entity(), "home-rows", sizes, move |app, range, window, cx| {
        let line_text = LineText::new(window, cx);
        range.map(|ix| row(app, &app.workspaces[rows[ix]], &line_text, cx)).collect::<Vec<_>>()
    });
    v_flex()
        .region(Region::Home)
        .size_full()
        .child(bar::bar(app, inset, cx))
        .when(empty, |d| d.child(nothing_listed(cx)))
        .when(!empty, |d| d.child(list.track_scroll(&home.scroll).flex_1().min_h_0()))
}

/// A workspace's row: its mark and name, its repository, and a button to archive it or bring it back, which says so
/// while it does. Clicking the rest of the row shows the workspace, as its row in the sidebar does, or brings an
/// archived one back. The repository and what the row says are smaller, on the name's baseline.
fn row(app: &AppView, workspace: &Workspace, line_text: &LineText, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme();
    let (border, hover, muted) = (theme.border, theme.list_hover, theme.muted_foreground);
    let Standing { signal, work, needs_a_look } = standing(app, workspace, cx);
    let repo = app.repo(&workspace.repo_id).map(|r| r.name.clone()).unwrap_or_default();
    let id = workspace.id.clone();
    h_flex()
        .id(ElementId::Name(format!("home-{}", workspace.id).into()))
        .w_full()
        .h(ROW_HEIGHT)
        .px_4()
        .gap_3()
        .border_b_1()
        .border_color(border)
        .hover(|d| d.bg(hover))
        .hand()
        .on_click(cx.listener(move |t, _, _, cx| t.open_workspace(id.clone(), cx)))
        .child(look::mark(signal, work, look::Size::Inline, cx))
        .child(
            line_text
                .body
                .set(div().flex_1().min_w_0().truncate())
                // As in the sidebar, what needs a look stands out, and everything else recedes.
                .when(needs_a_look, |d| d.font_weight(FontWeight::MEDIUM))
                .when(!needs_a_look, |d| d.text_color(muted))
                .child(workspace.label().to_string()),
        )
        .child(line_text.small.set(div().flex_none().w(REPO_WIDTH).truncate().text_color(muted).child(repo)))
        .child(row_button(app, workspace, &line_text.small, cx))
        .test_support()
        .into_any_element()
}

/// The button that archives the workspace or brings it back, or what the row says in its place while it does.
fn row_button(app: &AppView, workspace: &Workspace, small: &Placed, cx: &mut Context<AppView>) -> AnyElement {
    if let Some(waiting) = app.busy.doing(&workspace.id) {
        return doing(format!("home-doing-{}", workspace.id), waiting, small).into_any_element();
    }
    let archived = workspace.status == WorkspaceStatus::Archived;
    let (icon, what) = if archived { (Lucide::ArchiveRestore, "Restore") } else { (Lucide::Archive, "Archive") };
    let id = workspace.id.clone();
    Button::new(ElementId::Name(format!("home-{}-{}", what.to_lowercase(), workspace.id).into()))
        .ghost()
        .small()
        .icon(icon)
        .text_color(cx.theme().muted_foreground)
        .tooltip(what)
        .off_while(!app.busy.is_free())
        .on_click(cx.listener(move |t, _, _, cx| {
            // The row under it would take the click too, and show the workspace.
            cx.stop_propagation();
            if archived {
                t.restore(id.clone(), cx);
            } else {
                t.archive(id.clone(), false, cx);
            }
        }))
        .into_any_element()
}

fn nothing_listed(cx: &mut Context<AppView>) -> impl IntoElement {
    div()
        .id("home-nothing")
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .text_color(cx.theme().muted_foreground)
        .child("No workspaces to show")
        .test_support()
}

#[cfg(test)]
mod tests;
