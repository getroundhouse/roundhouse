//! Workspaces grouped by repository, each with its status.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu};
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Selectable as _, Sizable as _, h_flex, v_flex, v_virtual_list,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, FontWeight, IntoElement, ParentElement, Pixels, Rems, Size, Styled, Window,
    div, px, rems, size,
};
use look::{Signal, Work};
use protocol::model::{Repo, Session, SessionStatus, SetupStatus, Workspace, WorkspaceStatus};

use crate::app::AppView;
use crate::right_click;
use crate::workspace_pane::HEADER_HEIGHT;

mod footer;

/// Settings stands in for the sidebar at the same width.
pub(crate) const WIDTH: f32 = 260.0;
const REPO_HEIGHT: Pixels = px(26.);
/// A workspace's row, and the note in its place while one is on its way.
const ROW_HEIGHT: Pixels = px(30.);
/// The space below a line, before another line of the same repository.
const LINE_GAP: Pixels = px(1.);
/// The space below a repository's last line, before the next repository. In rems, as the rest of the sidebar's spacing
/// is, so it follows the theme's text size.
const GROUP_GAP: Rems = rems(1.);

/// One line of the list, by where its record is in the app's lists. The lines are made and drawn in the same frame,
/// and those lists can't change in between.
#[derive(Clone, Copy)]
enum Line {
    /// A repository's name, with the button for a new workspace in it.
    Repo(usize),
    Workspace(usize),
    /// A new workspace on its way, below its repository's others.
    Creating,
}

impl Line {
    fn height(self) -> Pixels {
        match self {
            Line::Repo(_) => REPO_HEIGHT,
            Line::Workspace(_) | Line::Creating => ROW_HEIGHT,
        }
    }
}

/// Every line, repository by repository, each one's workspaces in the order the hub lists them.
fn lines(app: &AppView) -> Vec<Line> {
    let mut lines = Vec::new();
    for (r, repo) in app.repos.iter().enumerate() {
        lines.push(Line::Repo(r));
        let listed = app.workspaces.iter().enumerate().filter(|(_, w)| w.repo_id == repo.id && app.lists(w));
        lines.extend(listed.map(|(w, _)| Line::Workspace(w)));
        if app.creating_in.as_ref() == Some(&repo.id) {
            lines.push(Line::Creating);
        }
    }
    lines
}

/// The space each line takes: its height and the gap below it, with `group_gap` before each repository after the
/// first. Only the height is used, since the list is as wide as the sidebar.
fn sizes(lines: &[Line], group_gap: Pixels) -> Vec<Size<Pixels>> {
    let gaps = lines.iter().skip(1).map(|next| match next {
        Line::Repo(_) => group_gap,
        Line::Workspace(_) | Line::Creating => LINE_GAP,
    });
    lines.iter().zip(gaps.chain([px(0.)])).map(|(line, gap)| size(px(0.), line.height() + gap)).collect()
}

fn line(app: &AppView, line: Line, cx: &mut Context<AppView>) -> AnyElement {
    match line {
        Line::Repo(r) => repo_header(app, &app.repos[r], cx),
        Line::Workspace(w) => workspace_row(app, &app.workspaces[w], cx),
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

fn workspace_row(app: &AppView, workspace: &Workspace, cx: &mut Context<AppView>) -> AnyElement {
    let selected = app.selected.as_ref() == Some(&workspace.id);
    let status = app.sessions.get(&workspace.id).and_then(|s| workspace_status(s));
    let archived = workspace.status == WorkspaceStatus::Archived;
    let id = workspace.id.clone();
    let muted = cx.theme().muted_foreground;
    let unread = !archived && !app.unread_chats(&workspace.id, cx).is_empty();
    let mark = if workspace.setup == SetupStatus::Running { Signal::SettingUp } else { signal(status, unread) };
    // Only a workspace whose pane is open knows its pull request, until the hub sends it with each workspace.
    let work = app.panes.get(&workspace.id).map_or(Work::NoPr, |pane| pane.read(cx).work());
    let menu = row_menu(app, workspace, archived, cx);
    let label = workspace.label().to_string();
    let row = Button::new(ElementId::Name(format!("workspace-{}", workspace.id).into()))
        .accessibility_label(label.clone())
        .ghost()
        .small()
        .w_full()
        .h(ROW_HEIGHT)
        .selected(selected)
        .on_click(cx.listener(move |t, _, _, cx| {
            if archived {
                t.restore(id.clone(), cx);
            } else {
                t.select(id.clone(), cx);
            }
        }))
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .text_sm()
                .when(selected || unread, |d| d.font_weight(FontWeight::MEDIUM))
                // Rows with nothing to read recede, so those with a reply stand out. The selected one is what's showing.
                .when(archived || !(selected || unread), |d| d.text_color(muted))
                .child(look::mark(mark, work, look::Size::Row, cx))
                .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(label))
                .when(archived, |d| d.child(div().flex_none().text_xs().text_color(look::faint()).child("Restore"))),
        );
    row.context_menu(menu).into_any_element()
}

/// What a right-click on a workspace's row offers. Marking its chats needs one with a reply, and an archived
/// workspace has none to show.
fn row_menu(
    app: &AppView,
    workspace: &Workspace,
    archived: bool,
    cx: &mut Context<AppView>,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    let marking = if archived { None } else { app.workspace_marking(&workspace.id, cx) };
    let (this, id) = (cx.weak_entity(), workspace.id.clone());
    let to_mark = this.clone();
    let marking = marking.map(|(marking, chats)| {
        let mark = move |cx: &mut App| {
            let _ = to_mark.update(cx, |t, cx| t.mark_chats(marking, chats.clone(), cx));
        };
        (marking, mark)
    });
    right_click::menu(marking, move |window, cx| {
        let _ = this.update(cx, |t, cx| t.start_renaming(id.clone(), window, cx));
    })
}

fn repo_header(app: &AppView, repo: &Repo, cx: &mut Context<AppView>) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let repo_id = repo.id.clone();
    let creating = app.creating_in.as_ref() == Some(&repo.id);
    h_flex()
        .w_full()
        .h(REPO_HEIGHT)
        .pl_2()
        .justify_between()
        .child(
            h_flex()
                .gap_1p5()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(muted)
                .child(Icon::new(IconName::Folder).xsmall().text_color(look::faint()))
                .child(repo.name.clone()),
        )
        .child(
            Button::new(ElementId::Name(format!("new-in-{}", repo.id).into()))
                .ghost()
                .xsmall()
                .icon(IconName::Plus)
                .tooltip("New workspace")
                .loading(creating)
                .on_click(cx.listener(move |t, _, window, cx| t.open_new_workspace(repo_id.clone(), window, cx))),
        )
        .into_any_element()
}

fn creating(cx: &mut Context<AppView>) -> AnyElement {
    div()
        .px_2()
        .h(ROW_HEIGHT)
        .flex()
        .items_center()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child("Creating a workspace…")
        .into_any_element()
}

pub(crate) fn render(app: &AppView, window: &Window, cx: &mut Context<AppView>) -> impl IntoElement {
    let theme = cx.theme();
    let (background, border) = (theme.sidebar, theme.sidebar_border);
    // A moving mark builds this view again on every frame, so only the lines on screen are built, and a long list
    // costs no more than a short one.
    let lines: Rc<[Line]> = lines(app).into();
    let sizes = Rc::new(sizes(&lines, GROUP_GAP.to_pixels(window.rem_size())));
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
        .child(list.track_scroll(&app.sidebar_scroll).flex_1().min_h_0().px_2())
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
            background_tasks: Vec::new(),
            queued: Vec::new(),
            queue_held: false,
            rev: protocol::model::Rev::default(),
            title_source: protocol::model::TitleSource::FirstLine,
        }
    }

    #[test]
    fn each_line_takes_its_height_and_the_gap_below_it() {
        let lines =
            [Line::Repo(0), Line::Workspace(0), Line::Workspace(1), Line::Creating, Line::Repo(1), Line::Workspace(2)];

        let heights: Vec<_> = sizes(&lines, px(14.)).iter().map(|s| s.height).collect();

        assert_eq!(heights, [px(27.), px(31.), px(31.), px(44.), px(27.), px(30.)]);
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
