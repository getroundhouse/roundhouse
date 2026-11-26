//! A line that says the app is waiting on something, such as a server it's asking or a device to pair: what it waits
//! for, muted, after a ring whose light goes round until the wait is over, as a background task's does while it runs.
//! Settings' panels and the dialogs that pair a device each draw their waits with it.

use gpui_kit::component::h_flex;
use gpui_kit::{App, Div, ParentElement as _, SharedString, Styled as _};
use look::TaskRing;

/// `said`, muted, after the turning ring, which holds still under Reduce Motion, and in pictures of the app.
pub(crate) fn waiting_line(said: impl Into<SharedString>, cx: &App) -> Div {
    h_flex()
        .gap(look::Space::Small)
        .text_color(look::colors(cx).text_muted)
        .child(look::task_ring(TaskRing::Running, cx))
        .child(said.into())
}
