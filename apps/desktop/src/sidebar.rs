//! Workspaces grouped by repository, each with its status.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use protocol::model::{Session, SessionStatus, Workspace, WorkspaceStatus};

use crate::app::AppView;

const WIDTH: f32 = 260.0;

/// The open chat that most needs attention speaks for the workspace.
pub(crate) fn workspace_status(sessions: &[Session]) -> Option<SessionStatus> {
    let urgency = |status: &SessionStatus| match status {
        SessionStatus::WaitingApproval => 3,
        SessionStatus::Running => 2,
        SessionStatus::Interrupted => 1,
        SessionStatus::Idle => 0,
    };
    sessions.iter().filter(|s| s.closed_at_ms.is_none()).map(|s| s.status).max_by_key(urgency)
}

pub(crate) fn status_color(status: Option<SessionStatus>, cx: &App) -> Hsla {
    let theme = cx.theme();
    match status {
        Some(SessionStatus::Running) => theme.primary,
        Some(SessionStatus::WaitingApproval) => theme.warning,
        Some(SessionStatus::Interrupted) => theme.danger,
        _ => theme.muted_foreground.opacity(0.4),
    }
}

fn workspace_row(app: &AppView, workspace: &Workspace, cx: &mut Context<AppView>) -> impl IntoElement {
    let selected = app.selected.as_ref() == Some(&workspace.id);
    let status = app.sessions.get(&workspace.id).and_then(|s| workspace_status(s));
    let archived = workspace.status == WorkspaceStatus::Archived;
    let id = workspace.id.clone();
    let muted = cx.theme().muted_foreground;
    let dot = status_color(status, cx);
    Button::new(ElementId::Name(format!("workspace-{}", workspace.id).into()))
        .ghost()
        .small()
        .w_full()
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
                .when(archived, |d| d.text_color(muted))
                .child(div().flex_none().size_2().rounded_full().bg(dot))
                .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().child(workspace.name.clone()))
                .when(archived, |d| d.child(div().flex_none().text_xs().child("Restore"))),
        )
}

pub(crate) fn render(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let theme = cx.theme();
    let (background, border, muted) = (theme.sidebar, theme.border, theme.muted_foreground);
    let visible = app.visible_workspaces();
    let groups: Vec<AnyElement> = app
        .repos
        .iter()
        .map(|repo| {
            let repo_id = repo.id.clone();
            let creating = app.creating_in.as_ref() == Some(&repo.id);
            let rows: Vec<_> = visible
                .iter()
                .filter(|w| w.repo_id == repo.id)
                .map(|w| workspace_row(app, w, cx).into_any_element())
                .collect();
            v_flex()
                .w_full()
                .gap_1()
                .child(
                    h_flex()
                        .w_full()
                        .px_2()
                        .justify_between()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(muted)
                                .child(repo.name.clone()),
                        )
                        .child(
                            Button::new(ElementId::Name(format!("new-in-{}", repo.id).into()))
                                .ghost()
                                .xsmall()
                                .label("New")
                                .loading(creating)
                                .disabled(app.busy)
                                .on_click(cx.listener(move |t, _, _, cx| t.new_workspace(repo_id.clone(), cx))),
                        ),
                )
                .children(rows)
                .when(creating, |d| d.child(div().px_2().text_xs().text_color(muted).child("Creating a workspace…")))
                .into_any_element()
        })
        .collect();
    let show_archived = app.show_archived;
    v_flex()
        .flex_none()
        .w(px(WIDTH))
        .h_full()
        .bg(background)
        .border_r_1()
        .border_color(border)
        .child(v_flex().id("workspaces").flex_1().min_h_0().overflow_y_scroll().p_2().pt_8().gap_4().children(groups))
        .child(
            h_flex()
                .flex_none()
                .p_2()
                .gap_3()
                .border_t_1()
                .border_color(border)
                .child(
                    Button::new("add-repository")
                        .ghost()
                        .xsmall()
                        .label("Add repository")
                        .on_click(cx.listener(|t, _, _, cx| t.add_repository(cx))),
                )
                .child(
                    Button::new("toggle-archived")
                        .ghost()
                        .xsmall()
                        .label(if show_archived { "Hide archived" } else { "Show archived" })
                        .on_click(cx.listener(|t, _, _, cx| {
                            t.show_archived = !t.show_archived;
                            cx.notify();
                        })),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::workspace_status;
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
}
