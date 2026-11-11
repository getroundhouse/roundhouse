//! Workspaces grouped by repository, each with its status, under a heading with the Add project button.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, IconNamed, Sizable as _, h_flex, v_flex, v_virtual_list};
use gpui_kit::{
    AnyElement, App, Context, Div, ElementId, FontWeight, Hsla, IntoElement, ParentElement, Pixels, Size, Styled, div,
    px, size,
};
use look::{Hand as _, LineText, Off as _};
use look::{Signal, Work};
use protocol::model::{Repo, SetupStatus, Workspace, WorkspaceStatus};

use crate::app::{AppView, Connection};
use crate::workspace_pane::HEADER_HEIGHT;

mod footer;
mod nav;
mod row;
pub(crate) mod signal;

pub(crate) use row::doing;

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
    /// A new workspace on its way, below its repository's others.
    Creating,
}

impl Line {
    fn height(self) -> Pixels {
        match self {
            Line::Heading => HEADING_HEIGHT,
            Line::Repo(_) => REPO_HEIGHT,
            Line::Workspace(_) | Line::Creating => ROW_HEIGHT,
        }
    }

    /// The space between this line and the one after it.
    fn gap_before(self, next: Line) -> Pixels {
        match (self, next) {
            (Line::Heading, _) | (Line::Workspace(_) | Line::Creating, Line::Workspace(_) | Line::Creating) => LINE_GAP,
            (_, Line::Repo(_) | Line::Heading) => GROUP_GAP,
            (Line::Repo(_), Line::Workspace(_) | Line::Creating) => UNDER_REPO,
        }
    }
}

/// Every line, repository by repository, each one's workspaces in the order the hub lists them, under the heading. The
/// heading shows before there's a repository too, since it has the button that adds one, but only once there's a hub
/// to add it to. Repositories listed while the hub is away, such as while it restarts, keep it over them.
fn lines(app: &AppView) -> Vec<Line> {
    let heading = connected(app) || !app.repos.is_empty();
    let mut lines = if heading { vec![Line::Heading] } else { Vec::new() };
    for (r, repo) in app.repos.iter().enumerate() {
        lines.push(Line::Repo(r));
        let listed = app.workspaces.iter().enumerate().filter(|(_, w)| w.repo_id == repo.id && lists(w));
        lines.extend(listed.map(|(w, _)| Line::Workspace(w)));
        if app.busy.creating_in() == Some(&repo.id) {
            lines.push(Line::Creating);
        }
    }
    lines
}

/// Whether the sidebar lists this workspace. An archived one is only in Home, which can show it and bring it back.
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

fn line(app: &AppView, line: Line, line_text: &LineText, cx: &mut Context<AppView>) -> AnyElement {
    match line {
        Line::Heading => heading(connected(app), cx),
        Line::Repo(r) => repo_header(app, &app.repos[r], cx),
        Line::Workspace(w) => row::workspace_row(app, &app.workspaces[w], line_text, cx),
        Line::Creating => creating(cx),
    }
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

/// What the list is, small and faint, as a section's label, and at its right end the Add project button, in words, so
/// it isn't taken for a repository's button for a new workspace under it. Greyed out while there's no hub to add to.
fn heading(connected: bool, cx: &mut Context<AppView>) -> AnyElement {
    let faint = cx.theme().muted_foreground.opacity(0.7);
    h_flex()
        .w_full()
        .h(HEADING_HEIGHT)
        .px_1p5()
        .child(div().flex_1().text_xs().font_weight(FontWeight::MEDIUM).text_color(faint).child("Workspaces"))
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
        .into_any_element()
}

/// Level with the names of the workspaces above it.
fn creating(cx: &mut Context<AppView>) -> AnyElement {
    div()
        .pl(ROW_INSET + look::Size::Row.width() + MARK_GAP)
        .h(ROW_HEIGHT)
        .flex()
        .items_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child("Creating a workspace…")
        .into_any_element()
}

/// The sidebar, filling the frame `sides` puts it in.
pub(crate) fn render(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let theme = cx.theme();
    let (background, border) = (theme.sidebar, theme.sidebar_border);
    // A moving mark builds this view again on every frame, so only the lines on screen are built, and a long list
    // costs no more than a short one.
    let lines: Rc<[Line]> = lines(app).into();
    let sizes = Rc::new(sizes(&lines));
    let list = v_virtual_list(cx.entity(), "workspaces", sizes, move |app, range, window, cx| {
        let line_text = LineText::new(window, cx);
        range.map(|ix| line(app, lines[ix], &line_text, cx)).collect::<Vec<_>>()
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
        .child(list.track_scroll(&app.sidebar_scroll).flex_1().min_h_0().px_1p5().pt_1p5())
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
            Line::Creating,
            Line::Repo(1),
            Line::Repo(2),
            Line::Workspace(2),
        ];

        let heights: Vec<_> = sizes(&lines).iter().map(|s| s.height).collect();

        assert_eq!(heights, [px(32.), px(38.), px(40.), px(40.), px(44.), px(40.), px(38.), px(36.)]);
    }
}
