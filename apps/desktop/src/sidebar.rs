//! Workspaces grouped by repository, each with its status, under a heading with the Add project button.

use std::rc::Rc;
use std::time::Instant;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, IconNamed, Sizable as _, h_flex, v_flex, v_virtual_list};
use gpui_kit::{
    AnyElement, App, Context, Div, ElementId, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement,
    Pixels, Size, Styled, TestSupportExt as _, Window, div, px, size,
};
use look::{Hand as _, LineText};
use look::{Signal, Work};
use protocol::model::{Repo, RepoId, SetupStatus, Workspace, WorkspaceId, WorkspaceStatus};

use crate::app::{AppView, Connection};
use crate::workspace_order::Sort;
use crate::workspace_pane::HEADER_HEIGHT;

mod footer;
mod heading;
mod kept;
mod nav;
mod row;
pub(crate) mod signal;

pub(crate) use kept::Kept;
pub(crate) use row::doing;

/// The orders the sidebar can list each project's workspaces in, in the order its heading's menu offers them.
pub(crate) const SORTS: [Sort; 2] = [Sort::LastActive, Sort::Oldest];

/// The heading over the projects, and over Settings' pages.
pub(crate) const HEADING_HEIGHT: Pixels = px(28.);
const REPO_HEIGHT: Pixels = px(32.);
/// A workspace's row, and the note in its place while one is on its way.
const ROW_HEIGHT: Pixels = px(36.);
/// How far a workspace's row sets its mark in from a repository's, so the rows read as the repository's.
const ROW_INSET: Pixels = px(12.);
/// The space between a mark and the name beside it.
const MARK_GAP: Pixels = px(6.);
/// The space between lines of one repository.
const LINE_GAP: Pixels = px(4.);
/// The space between a repository's name and its first workspace.
const UNDER_REPO: Pixels = px(6.);
/// The space between one repository's last line and the next repository.
const GROUP_GAP: Pixels = px(8.);

/// One line of the list, by where its record is in the app's lists. The lines are made and drawn in the same frame,
/// and those lists can't change in between.
#[derive(Clone, Copy)]
enum Line {
    /// What the list is, with the Add project button, above the first repository.
    Heading,
    /// A repository's name, with the button for a new workspace in it.
    Repo(usize),
    Workspace(usize),
    /// A workspace the hub is still making, wherever it was asked for, as a note in the place it will have.
    Creating(usize),
}

/// What a line shows, which stays the same while its place in the list changes, so it slides there. A workspace on its
/// way and the workspace once it's made are one line, so its row takes the note's place, even while the order holds.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum LineId {
    Heading,
    Repo(RepoId),
    Workspace(WorkspaceId),
}

impl Line {
    fn height(self) -> Pixels {
        match self {
            Line::Heading => HEADING_HEIGHT,
            Line::Repo(_) => REPO_HEIGHT,
            Line::Workspace(_) | Line::Creating(_) => ROW_HEIGHT,
        }
    }

    fn id(self, app: &AppView) -> LineId {
        match self {
            Line::Heading => LineId::Heading,
            Line::Repo(r) => LineId::Repo(app.repos[r].id.clone()),
            Line::Workspace(w) | Line::Creating(w) => LineId::Workspace(app.workspaces[w].id.clone()),
        }
    }

    /// The space between this line and the one after it.
    fn gap_before(self, next: Line) -> Pixels {
        match (self, next) {
            (Line::Heading, _) | (Line::Workspace(_) | Line::Creating(_), Line::Workspace(_) | Line::Creating(_)) => {
                LINE_GAP
            }
            (_, Line::Repo(_) | Line::Heading) => GROUP_GAP,
            (Line::Repo(_), Line::Workspace(_) | Line::Creating(_)) => UNDER_REPO,
        }
    }
}

/// Every line, repository by repository, each one's workspaces in `sort`'s order, under the heading. The heading shows
/// before there's a repository too, since it has the button that adds one, but only once there's a hub to add it to.
/// Repositories listed while the hub is away, such as while it restarts, keep it over them. A workspace the hub is
/// still making is a note in the place it will have, which its row takes once it's made. While someone is using the
/// list, at `now`, the workspaces stay in the order they were in, as [`Hold::held`](crate::reorder::Hold::held) says,
/// and any that come meanwhile go after them.
fn lines(app: &AppView, sort: Sort, now: Instant) -> Vec<Line> {
    let heading = connected(app) || !app.repos.is_empty();
    let mut lines = if heading { vec![Line::Heading] } else { Vec::new() };
    let held = app.sidebar.hold.held(&sort, now);
    let workspaces = &app.workspaces;
    let line = |w: usize| match workspaces[w].status {
        WorkspaceStatus::Ready => Some(Line::Workspace(w)),
        WorkspaceStatus::Creating => Some(Line::Creating(w)),
        WorkspaceStatus::Archived | WorkspaceStatus::Unknown => None,
    };
    for (r, repo) in app.repos.iter().enumerate() {
        lines.push(Line::Repo(r));
        let mut listed: Vec<usize> =
            (0..workspaces.len()).filter(|&w| workspaces[w].repo_id == repo.id && line(w).is_some()).collect();
        sort.sort(&mut listed, workspaces);
        if let Some(held) = held {
            crate::reorder::keep_held(&mut listed, workspaces, held);
        }
        lines.extend(listed.into_iter().filter_map(line));
    }
    lines
}

/// The lines in this frame, how far each is from its place on its way there, and the space each takes.
pub(crate) struct Arranged {
    lines: Rc<[Line]>,
    offsets: Rc<[Pixels]>,
    sizes: Rc<Vec<Size<Pixels>>>,
}

/// Works out the lines in the order the sidebar is in, and sends each to its place, so one that has moved slides
/// there. Done on every frame, whether or not the sidebar shows, so a line that moved while it was hidden is in its
/// place by the time it shows. Where each line has got to is read here, once for the frame, and asks for the next
/// frame while one moves.
pub(crate) fn arrange(app: &mut AppView, window: &mut Window, cx: &App) -> Arranged {
    let sort = crate::settings::get(cx).sidebar_sort;
    let lines: Rc<[Line]> = lines(app, sort, cx.background_executor().now()).into();
    let sizes = sizes(&lines);
    let ids: Vec<LineId> = lines.iter().map(|line| line.id(app)).collect();
    let drawn =
        ids.iter().filter_map(|id| if let LineId::Workspace(id) = id { Some(id.clone()) } else { None }).collect();
    let offsets = app.sidebar.places.arrange(ids.into_iter().zip(sizes.iter().map(|s| s.height)), window, cx);
    app.sidebar.hold.showed(sort, drawn);
    Arranged { lines, offsets, sizes: Rc::new(sizes) }
}

/// The workspaces the sidebar lists, in its order, which is the order the keys that move between them go in.
pub(crate) fn listed(app: &AppView, cx: &App) -> Vec<WorkspaceId> {
    let sort = crate::settings::get(cx).sidebar_sort;
    let workspace = |line: Line| if let Line::Workspace(w) = line { Some(app.workspaces[w].id.clone()) } else { None };
    lines(app, sort, cx.background_executor().now()).into_iter().filter_map(workspace).collect()
}

/// Whether the sidebar lists this workspace, to be opened. An archived one is only in Home, which can show it and bring
/// it back, and one being made shows only as a note until it's ready.
pub(crate) fn lists(workspace: &Workspace) -> bool {
    workspace.status == WorkspaceStatus::Ready
}

/// The space each line takes: its height and the gap below it. Only the height is used, since the list is as wide as
/// the sidebar.
fn sizes(lines: &[Line]) -> Vec<Size<Pixels>> {
    let gaps = lines.windows(2).map(|pair| pair[0].gap_before(pair[1]));
    lines.iter().zip(gaps.chain([px(0.)])).map(|(line, gap)| size(px(0.), line.height() + gap)).collect()
}

fn connected(app: &AppView) -> bool {
    matches!(app.connection, Connection::Ready(_))
}

/// The line, drawn `offset` from its place, which the list lays it out in, as it slides there.
fn line(app: &AppView, line: Line, offset: Pixels, line_text: &LineText, cx: &mut Context<AppView>) -> AnyElement {
    let drawn = match line {
        Line::Heading => heading::heading(app, connected(app), cx),
        Line::Repo(r) => repo_header(app, &app.repos[r], cx),
        Line::Workspace(w) => row::workspace_row(app, &app.workspaces[w], line_text, cx),
        Line::Creating(w) => creating(&app.workspaces[w], cx),
    };
    crate::reorder::offset_by(drawn, offset)
}

/// Where a workspace stands, as its row shows it, in the sidebar or in Home.
pub(crate) struct Standing {
    pub(crate) signal: Signal,
    pub(crate) work: Work,
    /// It needs a look, so its name stands out: it has a reply nobody has read, or a chat waiting on an answer or an
    /// approval.
    pub(crate) needs_a_look: bool,
}

pub(crate) fn standing(app: &AppView, workspace: &Workspace, cx: &App) -> Standing {
    let chats = app.sessions.get(&workspace.id).map_or(&[][..], |chats| chats);
    let unread = workspace.status != WorkspaceStatus::Archived && !app.unread_chats(&workspace.id, cx).is_empty();
    let needs_a_look = unread || signal::waiting_on_you(chats);
    let signal = if workspace.setup == SetupStatus::Running { Signal::SettingUp } else { signal::workspace(chats) };
    // The hub keeps it on the record, open or not. A hub too old to do that leaves it to the pane, if it's open.
    let work = crate::pull_request::recorded_work(workspace)
        .or_else(|| app.panes.get(&workspace.id).map(|pane| pane.read(cx).work()))
        .unwrap_or(Work::NoPr);
    Standing { signal, work, needs_a_look }
}

#[cfg(feature = "testing")]
impl AppView {
    /// What a workspace's mark shows of its pull request, in the sidebar or in Home.
    pub fn work_shown(&self, id: &protocol::model::WorkspaceId, cx: &App) -> Option<Work> {
        let workspace = self.workspaces.iter().find(|w| &w.id == id)?;
        Some(standing(self, workspace, cx).work)
    }
}

/// An icon centred in a box as wide as a line of body text is tall, as the sidebar's items and repositories have.
pub(super) fn icon_box(icon: impl IconNamed, color: Hsla) -> Div {
    let icon = Icon::new(icon).size_3p5().text_color(color);
    div().flex_none().size_4().flex().items_center().justify_center().child(icon)
}

fn repo_header(app: &AppView, repo: &Repo, cx: &mut Context<AppView>) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let repo_id = repo.id.clone();
    let creating = app.busy.creating_in() == Some(&repo.id);
    h_flex()
        .w_full()
        .h(REPO_HEIGHT)
        .px_1p5()
        .gap_2()
        .child(icon_box(IconName::Folder, look::faint()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(muted)
                .child(repo.name.clone()),
        )
        .child(
            Button::new(ElementId::Name(format!("new-in-{}", repo.id).into()))
                .ghost()
                .small()
                .icon(IconName::Plus)
                .text_color(muted)
                .tooltip("New workspace")
                .loading(creating)
                .hand_while(!creating)
                .on_click(cx.listener(move |t, _, window, cx| t.open_new_workspace(repo_id.clone(), window, cx))),
        )
        .into_any_element()
}

/// Level with the names of the workspaces above it.
fn creating(workspace: &Workspace, cx: &mut Context<AppView>) -> AnyElement {
    div()
        .id(ElementId::Name(format!("creating-{}", workspace.id).into()))
        .pl(ROW_INSET + look::Size::Row.width() + MARK_GAP)
        .h(ROW_HEIGHT)
        .flex()
        .items_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child("Creating a workspace…")
        .test_support()
        .into_any_element()
}

/// The sidebar, filling the frame `sides` puts it in, with the lines `arrange` worked out for this frame.
pub(crate) fn render(app: &AppView, arranged: Arranged, cx: &mut Context<AppView>) -> impl IntoElement {
    let theme = cx.theme();
    let (background, border) = (theme.sidebar, theme.sidebar_border);
    // A moving mark builds this view again on every frame, so only the lines on screen are built, and a long list
    // costs no more than a short one.
    let Arranged { lines, offsets, sizes } = arranged;
    let list = v_virtual_list(cx.entity(), "workspaces", sizes, move |app, range, window, cx| {
        let line_text = LineText::new(window, cx);
        range.map(|ix| line(app, lines[ix], offsets[ix], &line_text, cx)).collect::<Vec<_>>()
    });
    v_flex()
        .size_full()
        .bg(background)
        .border_r_1()
        .border_color(border)
        // Where the window's buttons sit, level with the workspace header, with the button that hides the sidebar
        // beside them. It stays put, so the list scrolls under its bottom edge rather than under the buttons.
        .child(div().flex_none().h(HEADER_HEIGHT))
        .child(nav::nav(app, cx))
        .child(
            crate::reorder::hears_the_pointer(
                div().id("workspaces-list").flex_1().min_h_0().flex().flex_col(),
                |t| &mut t.sidebar.hold,
                cx,
            )
            .child(list.track_scroll(&app.sidebar_scroll).flex_1().min_h_0().px_1p5().pt_1p5()),
        )
        .child(footer::footer(app, cx))
}

#[cfg(test)]
mod tests {
    use super::{Line, sizes};
    use gpui_kit::px;

    #[test]
    fn each_line_takes_its_height_and_the_gap_below_it() {
        let lines = [
            Line::Heading,
            Line::Repo(0),
            Line::Workspace(0),
            Line::Workspace(1),
            Line::Creating(2),
            Line::Repo(1),
            Line::Repo(2),
            Line::Workspace(3),
        ];

        let heights: Vec<_> = sizes(&lines).iter().map(|s| s.height).collect();

        assert_eq!(heights, [px(32.), px(38.), px(40.), px(40.), px(44.), px(40.), px(38.), px(36.)]);
    }
}
