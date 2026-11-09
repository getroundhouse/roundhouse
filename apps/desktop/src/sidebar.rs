//! Workspaces grouped by repository, each with its status.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Selectable as _, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, TestSupportExt as _, div, px,
};
use look::{IconSize, IconStyle, Signal, Work};
use protocol::model::{Repo, Session, SessionStatus, SetupStatus, Workspace, WorkspaceStatus};

use crate::app::{AppView, BuildLine};
use crate::updates::{Kind, Problem};
use crate::workspace_pane::HEADER_HEIGHT;

const WIDTH: f32 = 260.0;

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

pub(crate) fn signal(status: Option<SessionStatus>) -> Signal {
    match status {
        Some(SessionStatus::Running) => Signal::Working,
        Some(SessionStatus::WaitingApproval) => Signal::NeedsYou,
        Some(SessionStatus::Interrupted) => Signal::Failed,
        None | Some(SessionStatus::Idle | SessionStatus::Unknown) => Signal::Idle,
    }
}

fn workspace_row(app: &AppView, workspace: &Workspace, cx: &mut Context<AppView>) -> impl IntoElement {
    let selected = app.selected.as_ref() == Some(&workspace.id);
    let status = app.sessions.get(&workspace.id).and_then(|s| workspace_status(s));
    let archived = workspace.status == WorkspaceStatus::Archived;
    let id = workspace.id.clone();
    let muted = cx.theme().muted_foreground;
    let mark = if workspace.setup == SetupStatus::Running { Signal::SettingUp } else { signal(status) };
    // Only a workspace whose pane is open knows its pull request, until the hub sends it with each workspace.
    let work = app.panes.get(&workspace.id).map_or(Work::NoPr, |pane| pane.read(cx).work());
    Button::new(ElementId::Name(format!("workspace-{}", workspace.id).into()))
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
                .when(selected, |d| d.font_weight(FontWeight::MEDIUM))
                .when(archived, |d| d.text_color(muted))
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
        )
}

fn repo_group(app: &AppView, repo: &Repo, visible: &[&Workspace], cx: &mut Context<AppView>) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let repo_id = repo.id.clone();
    let creating = app.creating_in.as_ref() == Some(&repo.id);
    let rows: Vec<_> =
        visible.iter().filter(|w| w.repo_id == repo.id).map(|w| workspace_row(app, w, cx).into_any_element()).collect();
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

fn footer_item(id: &'static str, icon: IconName, label: &'static str) -> Button {
    Button::new(id).ghost().small().icon(icon).label(label)
}

/// What went wrong updating, under the build, with the detail in its own words on hover. A download that isn't what
/// the release workflow signed is the one that's red, since it may be someone tampering with the update.
fn update_problem(problem: &Problem, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let color = if matches!(problem.kind, Kind::Refused(_)) { theme.danger } else { theme.muted_foreground };
    let detail = problem.detail.clone();
    h_flex()
        .id("update-problem")
        .px_2()
        .gap_1p5()
        .text_xs()
        .text_color(color)
        .child(Icon::new(IconName::TriangleAlert).xsmall())
        .child(problem.summary())
        .tooltip(move |window, cx| Tooltip::new(detail.clone()).build(window, cx))
        .test_support()
        .into_any_element()
}

/// The version, faint, and the hub's too when it's another build the app leaves alone.
fn version(line: &BuildLine) -> AnyElement {
    let text = div().id("build-line").px_2().pt_1().text_xs().text_color(look::faint());
    match line {
        BuildLine::OtherHub { app, hub } => {
            let detail = format!(
                "The hub is {hub}, and this app is {app}. The app leaves a hub someone started themselves for them to \
                 update, and never restarts a newer one."
            );
            text.child(format!("{app} · hub {hub}"))
                .tooltip(move |window, cx| Tooltip::new(detail.clone()).build(window, cx))
                .test_support()
                .into_any_element()
        }
        BuildLine::Current { app } | BuildLine::UpdateReady { app, .. } | BuildLine::Finishing { app, .. } => {
            text.child(app.clone()).test_support().into_any_element()
        }
    }
}

/// Where an update has got to, when there's something to say, as a card with the app's icon in the version's place,
/// which says the version on hover. Installing an update is the card's main action. Finishing one stops agents, and
/// happens by itself, so it's a quiet button under what it's waiting on.
fn update_card(line: &BuildLine, cx: &mut Context<AppView>) -> Option<AnyElement> {
    let theme = cx.theme();
    let (surface, edge, muted) = (theme.secondary, theme.border, theme.muted_foreground);
    let text = |title: &str, detail: String, hover: String| {
        v_flex()
            .id("update-about")
            .min_w_0()
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title.to_string()))
            .child(div().text_xs().text_color(muted).child(detail))
            .tooltip(move |window, cx| Tooltip::new(hover.clone()).build(window, cx))
    };
    let (about, action) = match line {
        BuildLine::UpdateReady { app, next } => (
            text(
                "Update ready",
                format!("Roundhouse {next}"),
                format!(
                    "Roundhouse {next} is downloaded, and this is {app}. Restarting installs it, and agents keep \
                     running. Quitting installs it too."
                ),
            ),
            Some(
                Button::new("restart-to-update")
                    .primary()
                    .small()
                    .w_full()
                    .label("Restart to update")
                    .on_click(cx.listener(|t, _, window, cx| t.restart_to_update(window, cx))),
            ),
        ),
        BuildLine::Finishing { app, hub, agents } => {
            let hover = if *agents == 0 {
                format!(
                    "This is {app}, and the previous version, {hub}, is still busy, such as with a setup script or \
                     the command line connected to it. The update finishes by itself once it's free."
                )
            } else {
                format!(
                    "This is {app}, and agents are still running on the previous version, {hub}. The update \
                     finishes by itself once they're done."
                )
            };
            // Under the text, and in line with it, past the button's own padding.
            let finish = h_flex().pt_1().ml(px(-3.5)).child(finish_now(cx));
            (text("Finishing update", waiting_on(*agents), hover).child(finish), None)
        }
        BuildLine::Current { .. } | BuildLine::OtherHub { .. } => return None,
    };
    let card = v_flex()
        .id("update-card")
        .w_full()
        .mt_2()
        .p_2p5()
        .gap_2p5()
        .rounded(px(10.))
        .bg(surface)
        .border_1()
        .border_color(edge)
        .child(h_flex().items_start().gap_2p5().child(look::icon(IconStyle::Working, IconSize::Px64, cx)).child(about))
        .when_some(action, ParentElement::child)
        .test_support();
    Some(card.into_any_element())
}

/// What finishing an update is waiting on.
fn waiting_on(agents: usize) -> String {
    match agents {
        0 => "Waiting on other work".to_string(),
        1 => "Waiting on 1 agent".to_string(),
        n => format!("Waiting on {n} agents"),
    }
}

/// A quiet button, since finishing an update now stops the agents it's waiting on, which the app says first.
fn finish_now(cx: &mut Context<AppView>) -> Button {
    let finish = cx.listener(|t, _, _, cx| t.offer_hub_restart(cx));
    Button::new("finish-update").ghost().xsmall().label("Finish now").on_click(finish)
}

fn footer(
    show_archived: bool,
    build: &BuildLine,
    checking: bool,
    update: Option<&Problem>,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let border = cx.theme().sidebar_border;
    let card = update_card(build, cx);
    let problem = update.map(|problem| update_problem(problem, cx));
    v_flex()
        .flex_none()
        .items_start()
        .p_2()
        .gap_px()
        .border_t_1()
        .border_color(border)
        .child(
            footer_item("add-repository", IconName::Plus, "Add repository")
                .on_click(cx.listener(|t, _, _, cx| t.add_repository(cx))),
        )
        .child(
            footer_item("toggle-archived", IconName::Inbox, if show_archived { "Hide archived" } else { "Archived" })
                .on_click(cx.listener(|t, _, _, cx| {
                    t.show_archived = !t.show_archived;
                    cx.notify();
                })),
        )
        .child(card.unwrap_or_else(|| version(build)))
        .when(checking, |d| {
            d.child(
                div()
                    .id("checking-for-updates")
                    .px_2()
                    .text_xs()
                    .text_color(look::faint())
                    .child("Checking for updates…")
                    .test_support(),
            )
        })
        .when_some(problem, ParentElement::child)
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
        .child(footer(
            app.show_archived,
            &app.build_line(cx),
            crate::updates::checking(cx),
            crate::updates::problem_shown(cx).cloned().as_ref(),
            cx,
        ))
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
}
