//! What every screen shares: the bar across the top, the line saying how the connection stands, and the status
//! mark for a workspace.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, FontWeight, IntoElement, ParentElement, Pixels, SharedString, Styled, div, px};
use look::Signal;
use protocol::model::{Session, SessionStatus, SetupStatus, Workspace};

/// Tall enough for a thumb, which is what iOS asks of anything tapped.
pub(crate) const TAP_HEIGHT: f32 = 44.;

/// The bar across the top: a button on each side, and a title with a line under it. `top` is the safe area,
/// so the bar starts below the status bar.
pub(crate) fn header(
    title: SharedString,
    subtitle: Option<SharedString>,
    left: Option<AnyElement>,
    right: Option<AnyElement>,
    top: Pixels,
    cx: &App,
) -> AnyElement {
    let side = |button: Option<AnyElement>| div().w(px(TAP_HEIGHT)).flex_shrink_0().children(button);
    h_flex()
        .w_full()
        .flex_shrink_0()
        .pt(top)
        .px_2()
        .pb_1()
        .gap_2()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(side(left))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .child(div().font_weight(FontWeight::SEMIBOLD).truncate().child(title))
                .when_some(subtitle, |this, subtitle| {
                    this.child(div().text_xs().text_color(cx.theme().muted_foreground).truncate().child(subtitle))
                }),
        )
        .child(side(right))
        .into_any_element()
}

/// A line of text in the middle of an empty screen, saying what's missing and what to do.
pub(crate) fn empty(title: impl Into<SharedString>, detail: impl Into<SharedString>, cx: &App) -> AnyElement {
    v_flex()
        .flex_1()
        .items_center()
        .justify_center()
        .gap_2()
        .px_8()
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title.into()))
        .child(div().text_sm().text_center().text_color(cx.theme().muted_foreground).child(detail.into()))
        .into_any_element()
}

/// The open chat that most needs attention speaks for the workspace, as in the desktop app's sidebar.
pub(crate) fn workspace_signal(workspace: &Workspace, sessions: &[Session]) -> Signal {
    if workspace.setup == SetupStatus::Running {
        return Signal::SettingUp;
    }
    let urgency = |status: &SessionStatus| match status {
        SessionStatus::WaitingApproval => 3,
        SessionStatus::Running => 2,
        SessionStatus::Interrupted => 1,
        SessionStatus::Idle | SessionStatus::Unknown => 0,
    };
    let status = sessions.iter().filter(|s| s.closed_at_ms.is_none()).map(|s| s.status).max_by_key(urgency);
    match status {
        Some(SessionStatus::Running) => Signal::Working,
        Some(SessionStatus::WaitingApproval) => Signal::NeedsYou,
        Some(SessionStatus::Interrupted) => Signal::Failed,
        None | Some(SessionStatus::Idle | SessionStatus::Unknown) => Signal::Idle,
    }
}

/// What to show under a workspace's name: its branch, unless that only says the name again, as the hub's own
/// choice of both does.
pub(crate) fn branch_under_name(workspace: &Workspace) -> Option<SharedString> {
    (workspace.branch != workspace.name).then(|| workspace.branch.clone().into())
}

/// The chat a workspace opens on: its most recently active open one.
pub(crate) fn chat_to_open(sessions: &[Session]) -> Option<&Session> {
    sessions.iter().filter(|s| s.closed_at_ms.is_none()).max_by_key(|s| s.updated_at_ms)
}
