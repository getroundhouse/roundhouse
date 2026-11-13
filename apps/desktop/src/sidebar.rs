//! Workspaces grouped by repository, each with its status, under a heading with the Add project button.

use std::ops::Range;
use std::rc::Rc;
use std::time::Instant;

use gpui_kit::component::{Icon, IconNamed, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement, Pixels,
    Styled, TestSupportExt as _, Window, div, px,
};
use look::LineText;
use protocol::model::{RepoId, Workspace, WorkspaceId, WorkspaceStatus};

use crate::app::{AppView, Connection};
use crate::reorder;
use crate::settings::Settings;
use crate::workspace_order::Sort;
use crate::workspace_pane::HEADER_HEIGHT;

mod drag;
mod fold;
mod footer;
mod heading;
mod kept;
mod nav;
mod project;
mod row;
pub(crate) mod signal;
mod standing;

use fold::{Drawn, Fold, Laid};
pub(crate) use kept::Kept;
pub(crate) use row::doing;
use standing::needs_a_look;
pub(crate) use standing::{Standing, standing};

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
/// The space around the list, at its sides and above its first line.
const LIST_PAD: Pixels = px(6.);

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
    /// Where the list ends, which no line is drawn at. The last project's workspaces fold away under what comes
    /// after them, as every project's do, and after them that's only this.
    End,
}

impl Line {
    fn height(self) -> Pixels {
        match self {
            Line::Heading => HEADING_HEIGHT,
            Line::Repo(_) => REPO_HEIGHT,
            Line::Workspace(_) | Line::Creating(_) => ROW_HEIGHT,
        }
    }

    /// Whether it's a workspace's line, which is under its project's row.
    fn is_workspace(self) -> bool {
        matches!(self, Line::Workspace(_) | Line::Creating(_))
    }

    fn id(self, app: &AppView) -> LineId {
        match self {
            Line::Heading => LineId::Heading,
            Line::Repo(r) => LineId::Repo(app.records().repos()[r].id.clone()),
            Line::Workspace(w) | Line::Creating(w) => LineId::Workspace(app.records().workspaces()[w].id.clone()),
        }
    }

    /// The project it's in, or is the row of.
    fn project(self, app: &AppView) -> Option<&RepoId> {
        match self {
            Line::Heading => None,
            Line::Repo(r) => Some(&app.records().repos()[r].id),
            Line::Workspace(w) | Line::Creating(w) => Some(&app.records().workspaces()[w].repo_id),
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

/// Every line, repository by repository in the order chosen, each one's workspaces in the order chosen, under the
/// heading. The heading shows before there's a repository too, since it has the button that adds one, but only once
/// there's a hub to add it to. Repositories listed while the hub is away, such as while it restarts, keep it over
/// them. A repository has its workspaces under it only where `shows` says, which it doesn't for one that's collapsed.
/// A workspace the hub is still making is a note in the place it will have, which its row takes once it's made. While
/// someone is using the list, at `now`, the workspaces stay in the order they were in, as
/// [`Hold::held`](crate::reorder::Hold::held) says, and any that come meanwhile go after them.
fn lines(app: &AppView, settings: &Settings, now: Instant, shows: impl Fn(&RepoId) -> bool) -> Vec<Line> {
    let sort = settings.sidebar_sort;
    let repos = app.records().repos();
    let heading = connected(app) || !repos.is_empty();
    let mut lines = if heading { vec![Line::Heading] } else { Vec::new() };
    let held = app.sidebar.hold.held(&sort, now);
    let workspaces = app.records().workspaces();
    let line = |w: usize| if lists(&workspaces[w]) { Line::Workspace(w) } else { Line::Creating(w) };
    for r in drag::ordered(repos, app.sidebar.project_order(settings)) {
        let repo = &repos[r];
        lines.push(Line::Repo(r));
        if !shows(&repo.id) {
            continue;
        }
        let mut listed: Vec<usize> =
            (0..workspaces.len()).filter(|&w| workspaces[w].repo_id == repo.id && has_line(&workspaces[w])).collect();
        sort.sort(&mut listed, workspaces);
        if let Some(held) = held {
            crate::reorder::keep_held(&mut listed, workspaces, held);
        }
        lines.extend(listed.into_iter().map(line));
    }
    lines
}

/// The lines in this frame, each as it's drawn, how far down the list the surface under a dragged project's lines
/// reaches, and how far down the list ends.
pub(crate) struct Arranged {
    lines: Rc<[Drawn]>,
    surface: Option<Range<Pixels>>,
    end: Pixels,
}

/// Works out the lines in the order the sidebar is in, and sends each to its place, so one that has moved slides
/// there. Done on every frame, whether or not the sidebar shows, so a line that moved while it was hidden is in its
/// place by the time it shows. Where each line has got to is read here, once for the frame, and asks for the next
/// frame while one moves. The workspaces of a project that's just been collapsed are still lines until they're
/// covered, and have no place to go to, so [`fold`] says where they are. A project being dragged is moved on first,
/// since it can change the order.
pub(crate) fn arrange(app: &mut AppView, window: &mut Window, cx: &mut App) -> Arranged {
    app.keep_up_with_a_drag(window, cx);
    let settings = crate::settings::get(cx);
    let (sort, collapsed) = (settings.sidebar_sort, &settings.collapsed_projects);
    let fold = |repo: &RepoId| Fold::of(collapsed.contains(repo), app.sidebar.folding.contains(repo));
    let lines = lines(app, &settings, cx.background_executor().now(), |repo| fold(repo).has_lines());
    let folds = fold::folds(&lines, |repo| fold(&app.records().repos()[repo].id));
    let ids: Vec<LineId> = lines.iter().map(|line| line.id(app)).collect();
    // With those on their way under their project's row, so the order they're in holds until they're there.
    let shown = ids.iter().filter_map(|id| if let LineId::Workspace(id) = id { Some(id.clone()) } else { None });
    let shown = shown.collect();
    let rests = |at: usize| fold::rests(lines[at], folds[at]);
    let resting: Vec<Line> = (0..lines.len()).filter(|&at| rests(at)).map(|at| lines[at]).collect();
    let places = (ids.into_iter().enumerate()).filter(|&(at, _)| rests(at)).map(|(_, id)| id).chain([LineId::End]);
    let sizes = sizes(&resting).into_iter().chain([px(0.)]);
    let sent = app.sidebar.places.arrange(places.zip(sizes), window, cx);
    app.sidebar.groups = drag::groups(&resting, &sent.places, |repo| app.records().repos()[repo].id.clone());
    let Laid { lines: mut drawn, end, settled } = fold::lay(&lines, &folds, &sent);
    let dragged = drawn.iter().position(|drawn| matches!(drawn.line, Line::Repo(_)) && lifted(app, drawn.line));
    let surface = app.sidebar.carry(&mut drawn, dragged);
    // Only the projects listed, so one that goes partway isn't kept.
    let still_folding = |drawn: &Drawn| match drawn.line {
        Line::Repo(repo) if drawn.fold.moving() && !settled.contains(&repo) => {
            Some(app.records().repos()[repo].id.clone())
        }
        _ => None,
    };
    app.sidebar.folding = drawn.iter().filter_map(still_folding).collect();
    app.sidebar.hold.showed(sort, shown);
    Arranged { lines: drawn.into(), surface, end }
}

/// The workspaces the sidebar shows, in its order, which is the order the keys that move between them go in. A
/// collapsed project's aren't among them.
pub(crate) fn shown(app: &AppView, cx: &App) -> Vec<WorkspaceId> {
    let collapsed = crate::settings::get(cx).collapsed_projects;
    in_order(app, cx, |repo| !collapsed.contains(repo))
}

/// The workspaces the sidebar lists, in its order, whether or not their projects are collapsed.
pub(crate) fn listed(app: &AppView, cx: &App) -> Vec<WorkspaceId> {
    in_order(app, cx, |_| true)
}

/// The workspaces of the projects that `shows` says for, in the sidebar's order.
fn in_order(app: &AppView, cx: &App, shows: impl Fn(&RepoId) -> bool) -> Vec<WorkspaceId> {
    let workspaces = app.records().workspaces();
    let workspace = |line: Line| if let Line::Workspace(w) = line { Some(workspaces[w].id.clone()) } else { None };
    let lines = lines(app, &crate::settings::get(cx), cx.background_executor().now(), shows);
    lines.into_iter().filter_map(workspace).collect()
}

/// Whether the sidebar lists this workspace, to be opened. An archived one is only in Home, which can show it and bring
/// it back, and one being made shows only as a note until it's ready. One being archived is still there until it's
/// gone.
pub(crate) fn lists(workspace: &Workspace) -> bool {
    matches!(workspace.status, WorkspaceStatus::Ready | WorkspaceStatus::Archiving)
}

/// Whether the sidebar has a line for this workspace: its row, or the note that it's on its way.
fn has_line(workspace: &Workspace) -> bool {
    lists(workspace) || workspace.status == WorkspaceStatus::Creating
}

/// The room each line takes down the list: its height and the gap below it.
fn sizes(lines: &[Line]) -> Vec<Pixels> {
    let gaps = lines.windows(2).map(|pair| pair[0].gap_before(pair[1]));
    lines.iter().zip(gaps.chain([px(0.)])).map(|(line, gap)| line.height() + gap).collect()
}

/// How far down the list each of `lines` goes at rest, and after them where the list ends, as
/// [`Places`](reorder::Places) sends them.
#[cfg(test)]
fn places(lines: &[Line]) -> Vec<Pixels> {
    let mut top = px(0.);
    let mut places = vec![top];
    places.extend(sizes(lines).into_iter().map(|size| {
        top += size;
        top
    }));
    places
}

fn connected(app: &AppView) -> bool {
    matches!(app.connection, Connection::Ready(_))
}

/// Whether the line is one of the project's that's being dragged, or is on its way to its place from where it was
/// let go, which are drawn over the others.
fn lifted(app: &AppView, line: Line) -> bool {
    app.sidebar.dragging.as_ref().is_some_and(|dragging| line.project(app) == Some(&dragging.project))
}

/// The line, cut off after as much as shows of it, for a workspace's row that its project's fold covers part of.
fn line(
    app: &AppView,
    Drawn { line, fold, cut, out, .. }: Drawn,
    line_text: &LineText,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let element = match line {
        Line::Heading => heading::heading(app, connected(app), cx),
        Line::Repo(r) => project::row(app, &app.records().repos()[r], fold.collapsed(), out, cx),
        Line::Workspace(w) => row::workspace_row(app, &app.records().workspaces()[w], line_text, window, cx),
        Line::Creating(w) => creating(&app.records().workspaces()[w], cx),
    };
    match cut {
        None => element,
        Some(_) => div().size_full().overflow_hidden().child(element).into_any_element(),
    }
}

/// An icon centred in a box as wide as a line of body text is tall, as the sidebar's items and repositories have.
pub(super) fn icon_box(icon: impl IconNamed, color: Hsla) -> Div {
    let icon = Icon::new(icon).size_3p5().text_color(color);
    div().flex_none().size_4().flex().items_center().justify_center().child(icon)
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
        .text_color(look::colors(cx).text_muted)
        .child("Creating a workspace…")
        .test_support()
        .into_any_element()
}

/// The sidebar, filling the frame `sides` puts it in, with the lines `arrange` worked out for this frame.
pub(crate) fn render(app: &AppView, arranged: Arranged, cx: &mut Context<AppView>) -> impl IntoElement {
    let colors = look::colors(cx);
    let (background, border) = (colors.sidebar, colors.edge);
    // A moving mark builds this view again on every frame, so only the lines on screen are built, and a long list
    // costs no more than a short one.
    let Arranged { lines, surface, end } = arranged;
    // A row has as much room as shows of it, which is none once what comes after its project has covered it.
    let place = |drawn: &Drawn| reorder::Drawn {
        top: drawn.top,
        height: drawn.cut.unwrap_or_else(|| drawn.line.height()),
        above: lifted(app, drawn.line),
    };
    // The surface first, so it's drawn under the lines it's for.
    let first_line = usize::from(surface.is_some());
    let places: Rc<[reorder::Drawn]> = surface.iter().map(drag::surface_drawn).chain(lines.iter().map(place)).collect();
    let list =
        reorder::list(cx.entity(), "workspaces", &app.sidebar_scroll, places, end, move |app, in_sight, window, cx| {
            let line_text = LineText::new(window, cx);
            let drawn = |&ix: &usize| match ix.checked_sub(first_line) {
                Some(at) => line(app, lines[at], &line_text, window, cx),
                None => drag::surface(cx),
            };
            in_sight.iter().map(drawn).collect()
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
            reorder::hears_the_pointer(
                div().id("workspaces-list").flex_1().min_h_0().flex().flex_col(),
                |t| &mut t.sidebar.hold,
                cx,
            )
            // A drag is told to every element that listens for it, wherever the pointer is, so a project dragged
            // past either end of the list still goes there.
            .on_drag_move(cx.listener(AppView::drag_project))
            .child(list.flex_1().min_h_0().px(LIST_PAD).pt(LIST_PAD))
            .test_support(),
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

        assert_eq!(sizes(&lines), [px(32.), px(38.), px(40.), px(40.), px(44.), px(40.), px(38.), px(36.)]);
    }
}
