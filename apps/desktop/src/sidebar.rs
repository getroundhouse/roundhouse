//! Workspaces grouped by repository, each with its status.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Selectable as _, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use look::{Signal, Work};
use protocol::model::{Repo, Session, SessionStatus, SetupStatus, Workspace, WorkspaceStatus};

use crate::app::AppView;
use crate::workspace_pane::HEADER_HEIGHT;

mod footer;

/// Settings stands in for the sidebar at the same width.
pub(crate) const WIDTH: f32 = 260.0;

/// The open chat that most needs attention speaks for the workspace.
pub(crate) fn workspace_status(sessions: &[Session]) -> Option<SessionStatus> {
    let urgency = |status: &SessionStatus| match status {
        SessionStatus::WaitingApproval => 3,
        SessionStatus::Running => 2,
        SessionStatus::Interrupted => 1,
        SessionStatus::Idle | SessionStatus::Unknown => 0,
    };
    sessions.iter().filter(|s| s.closed_at_ms.is_none()).map(|s| s.status).max_by_key(urgency)
}

/// A chat's or workspace's mark. A reply nobody has read only shows once nothing more pressing does: an agent at work
/// will reply again, and amber or red needs dealing with first.
pub(crate) fn signal(status: Option<SessionStatus>, unread: bool) -> Signal {
    match status {
        Some(SessionStatus::Running) => Signal::Working,
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
    let marking = if archived { None } else { app.workspace_marking(&workspace.id, cx) };
    let row = Button::new(ElementId::Name(format!("workspace-{}", workspace.id).into()))
        .ghost()
        .small()
        .w_full()
        .h(px(30.))
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
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(workspace.name.clone()),
                )
                .when(archived, |d| d.child(div().flex_none().text_xs().text_color(look::faint()).child("Restore"))),
        );
    let Some((marking, chats)) = marking else { return row.into_any_element() };
    let this = cx.weak_entity();
    row.context_menu(move |menu, _, _| {
        let (this, chats) = (this.clone(), chats.clone());
        menu.item(PopupMenuItem::new(format!("Mark as {}", marking.word())).on_click(move |_, _, cx| {
            let _ = this.update(cx, |t, cx| t.mark_chats(marking, chats.clone(), cx));
        }))
    })
    .into_any_element()
}

fn repo_group(app: &AppView, repo: &Repo, visible: &[&Workspace], cx: &mut Context<AppView>) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let repo_id = repo.id.clone();
    let creating = app.creating_in.as_ref() == Some(&repo.id);
    let rows: Vec<_> = visible.iter().filter(|w| w.repo_id == repo.id).map(|w| workspace_row(app, w, cx)).collect();
    v_flex()
        .w_full()
        .gap_px()
        .child(
            h_flex()
                .w_full()
                .h(px(26.))
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
                        .disabled(app.busy)
                        .on_click(cx.listener(move |t, _, _, cx| t.new_workspace(repo_id.clone(), cx))),
                ),
        )
        .children(rows)
        .when(creating, |d| {
            d.child(
                div()
                    .px_2()
                    .h(px(30.))
                    .flex()
                    .items_center()
                    .text_xs()
                    .text_color(muted)
                    .child("Creating a workspace…"),
            )
        })
        .into_any_element()
}

pub(crate) fn render(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let theme = cx.theme();
    let (background, border) = (theme.sidebar, theme.sidebar_border);
    let visible = app.visible_workspaces();
    let groups: Vec<AnyElement> = app.repos.iter().map(|repo| repo_group(app, repo, &visible, cx)).collect();
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
        .child(v_flex().id("workspaces").flex_1().min_h_0().overflow_y_scroll().px_2().gap_4().children(groups))
        .child(footer::footer(
            app.show_archived,
            &app.build_line(cx),
            app.update_question.as_ref(),
            crate::updates::checking(cx),
            crate::updates::answered(cx),
            crate::updates::problem_shown(cx).cloned().as_ref(),
            cx,
        ))
}

#[cfg(test)]
mod tests {
    use super::{signal, workspace_status};
    use look::Signal;
    use protocol::model::{AgentKind, Session, SessionStatus};

    fn session(status: SessionStatus, closed: bool) -> Session {
        Session {
            id: "s".into(),
            workspace_id: "w".into(),
            agent: AgentKind::Claude,
            model: None,
            permission_mode: None,
            status,
            last_seq: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
            title: None,
            closed_at_ms: closed.then_some(1),
            reply_seq: 0,
            read_seq: 0,
            rev: protocol::model::Rev::default(),
        }
    }

    #[test]
    fn the_chat_needing_attention_speaks_for_the_workspace_wherever_it_is() {
        use SessionStatus::*;
        let chats = [session(WaitingApproval, false), session(Running, false), session(Idle, false)];
        assert_eq!(workspace_status(&chats), Some(WaitingApproval));
        let chats = [session(Idle, false), session(Running, false), session(Interrupted, false)];
        assert_eq!(workspace_status(&chats), Some(Running));
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
        assert_eq!(signal(Some(WaitingApproval), true), Signal::NeedsYou);
        assert_eq!(signal(Some(Interrupted), true), Signal::Failed);
        assert_eq!(signal(Some(Idle), false), Signal::Idle);
    }
}
