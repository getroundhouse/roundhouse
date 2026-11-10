//! Workspaces grouped by repository, each with its status, under a heading with the Add project button.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, IconNamed, Sizable as _, h_flex, v_flex, v_virtual_list,
};
use gpui_kit::{
    AnyElement, App, Context, Div, ElementId, FontWeight, Hsla, IntoElement, ParentElement, Pixels, Size, Styled, div,
    px, size,
};
use look::{Signal, Work};
use protocol::model::{Repo, Session, SessionStatus, SetupStatus, Workspace, WorkspaceStatus};

use crate::app::{AppView, Connection};
use crate::workspace_pane::HEADER_HEIGHT;

mod footer;
mod nav;
mod row;

/// Its right edge included. As wide as the sidebar `docs/inspiration.md` measures, so the layout feels familiar.
/// Settings stands in for the sidebar at the same width.
pub(crate) const WIDTH: f32 = 290.0;
const HEADING_HEIGHT: Pixels = px(28.);
const REPO_HEIGHT: Pixels = px(32.);
/// A workspace's row, and the note in its place while one is on its way.
const ROW_HEIGHT: Pixels = px(36.);
/// How far a workspace's row sets its mark in from a repository's, so the rows read as the repository's.
const ROW_INSET: Pixels = px(20.);
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
        if app.creating_in.as_ref() == Some(&repo.id) {
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

fn line(app: &AppView, line: Line, cx: &mut Context<AppView>) -> AnyElement {
    match line {
        Line::Heading => heading(connected(app), cx),
        Line::Repo(r) => repo_header(app, &app.repos[r], cx),
        Line::Workspace(w) => row::workspace_row(app, &app.workspaces[w], cx),
        Line::Creating => creating(cx),
    }
}

/// The open chat that most needs attention speaks for the workspace.
pub(crate) fn workspace_status(sessions: &[Session]) -> Option<SessionStatus> {
    let urgency = |status: &SessionStatus| match status {
        SessionStatus::WaitingApproval => 3,
        SessionStatus::Running | SessionStatus::Background => 2,
        SessionStatus::Interrupted => 1,
        SessionStatus::Idle | SessionStatus::Unknown => 0,
    };
    sessions.iter().filter(|s| s.closed_at_ms.is_none()).map(|s| s.status).max_by_key(urgency)
}

/// A chat's or workspace's mark. A reply nobody has read only shows once nothing more pressing does: an agent at work
/// will reply again, and amber or red needs dealing with first.
pub(crate) fn signal(status: Option<SessionStatus>, unread: bool) -> Signal {
    match status {
        // Work left in the background picks the agent up again, so it's still at work.
        Some(SessionStatus::Running | SessionStatus::Background) => Signal::Working,
        Some(SessionStatus::WaitingApproval) => Signal::NeedsYou,
        Some(SessionStatus::Interrupted) => Signal::Failed,
        None | Some(SessionStatus::Idle | SessionStatus::Unknown) if unread => Signal::Unread,
        None | Some(SessionStatus::Idle | SessionStatus::Unknown) => Signal::Idle,
    }
}

/// Where a workspace stands, as its row shows it, in the sidebar or in Home.
pub(crate) struct Standing {
    pub(crate) signal: Signal,
    pub(crate) work: Work,
    /// It has a reply nobody has read, so its name stands out.
    pub(crate) unread: bool,
}

pub(crate) fn standing(app: &AppView, workspace: &Workspace, cx: &App) -> Standing {
    let status = app.sessions.get(&workspace.id).and_then(|s| workspace_status(s));
    let unread = workspace.status != WorkspaceStatus::Archived && !app.unread_chats(&workspace.id, cx).is_empty();
    let signal = if workspace.setup == SetupStatus::Running { Signal::SettingUp } else { signal(status, unread) };
    // Only a workspace whose pane is open knows its pull request, until the hub sends it with each workspace.
    let work = app.panes.get(&workspace.id).map_or(Work::NoPr, |pane| pane.read(cx).work());
    Standing { signal, work, unread }
}

/// An icon centred in a box as wide as a line of body text is tall, as the sidebar's items and repositories have.
pub(super) fn icon_box(icon: impl IconNamed, color: Hsla) -> Div {
    let icon = Icon::new(icon).size_3p5().text_color(color);
    div().flex_none().size_4().flex().items_center().justify_center().child(icon)
}

fn repo_header(app: &AppView, repo: &Repo, cx: &mut Context<AppView>) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let repo_id = repo.id.clone();
    let creating = app.creating_in.as_ref() == Some(&repo.id);
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
                .disabled(!connected)
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

pub(crate) fn render(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let theme = cx.theme();
    let (background, border) = (theme.sidebar, theme.sidebar_border);
    // A moving mark builds this view again on every frame, so only the lines on screen are built, and a long list
    // costs no more than a short one.
    let lines: Rc<[Line]> = lines(app).into();
    let sizes = Rc::new(sizes(&lines));
    let list = v_virtual_list(cx.entity(), "workspaces", sizes, move |app, range, _, cx| {
        range.map(|ix| line(app, lines[ix], cx)).collect::<Vec<_>>()
    });
    v_flex()
        .flex_none()
        .w(px(WIDTH))
        .h_full()
        .bg(background)
        .border_r_1()
        .border_color(border)
        // Where the window's buttons sit, level with the workspace header. It stays put, so the list scrolls
        // under its bottom edge rather than under the buttons.
        .child(div().flex_none().h(HEADER_HEIGHT))
        .child(nav::nav(app, cx))
        .child(list.track_scroll(&app.sidebar_scroll).flex_1().min_h_0().px_1p5().pt_1p5())
        .child(footer::footer(app, cx))
}

#[cfg(test)]
mod tests {
    use super::{Line, signal, sizes, workspace_status};
    use gpui_kit::px;
    use look::Signal;
    use protocol::model::{AgentKind, Session, SessionStatus};

    fn session(status: SessionStatus, closed: bool) -> Session {
        Session {
            id: "s".into(),
            workspace_id: "w".into(),
            agent: AgentKind::Claude,
            model: None,
            permission_mode: None,
            effort: None,
            status,
            last_seq: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
            title: None,
            closed_at_ms: closed.then_some(1),
            reply_seq: 0,
            read_seq: 0,
            turn_started_at_ms: None,
            background_tasks: Vec::new(),
            queued: Vec::new(),
            queue_held: false,
            rev: protocol::model::Rev::default(),
            title_source: protocol::model::TitleSource::FirstLine,
        }
    }

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

    #[test]
    fn the_chat_needing_attention_speaks_for_the_workspace_wherever_it_is() {
        use SessionStatus::*;
        let chats = [session(WaitingApproval, false), session(Running, false), session(Idle, false)];
        assert_eq!(workspace_status(&chats), Some(WaitingApproval));
        let chats = [session(Idle, false), session(Running, false), session(Interrupted, false)];
        assert_eq!(workspace_status(&chats), Some(Running));
        let chats = [session(Interrupted, false), session(Background, false), session(Idle, false)];
        assert_eq!(workspace_status(&chats), Some(Background));
    }

    #[test]
    fn a_closed_chat_doesnt_count() {
        use SessionStatus::*;
        assert_eq!(workspace_status(&[session(Running, true), session(Idle, false)]), Some(Idle));
        assert_eq!(workspace_status(&[session(Running, true)]), None);
    }

    #[test]
    fn a_reply_nobody_has_read_shows_only_once_nothing_more_pressing_does() {
        use SessionStatus::*;
        assert_eq!(signal(Some(Idle), true), Signal::Unread);
        assert_eq!(signal(None, true), Signal::Unread);
        assert_eq!(signal(Some(Running), true), Signal::Working);
        assert_eq!(signal(Some(Background), true), Signal::Working);
        assert_eq!(signal(Some(WaitingApproval), true), Signal::NeedsYou);
        assert_eq!(signal(Some(Interrupted), true), Signal::Failed);
        assert_eq!(signal(Some(Idle), false), Signal::Idle);
    }
}
