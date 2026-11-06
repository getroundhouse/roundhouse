//! Workspaces grouped by repository, each with its status.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use protocol::model::{SessionStatus, Workspace, WorkspaceStatus};

use crate::app::AppView;

const WIDTH: f32 = 260.0;

fn status_color(status: Option<SessionStatus>, cx: &App) -> Hsla {
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
    let status = app.sessions.get(&workspace.id).and_then(|s| s.last()).map(|s| s.status);
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

pub fn render(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let theme = cx.theme();
    let (background, border, muted) = (theme.sidebar, theme.border, theme.muted_foreground);
    let visible = app.visible_workspaces();
    let groups: Vec<AnyElement> = app
        .repos
        .iter()
        .map(|repo| {
            let repo_id = repo.id.clone();
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
                                .on_click(cx.listener(move |t, _, _, cx| t.new_workspace(repo_id.clone(), cx))),
                        ),
                )
                .children(rows)
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
