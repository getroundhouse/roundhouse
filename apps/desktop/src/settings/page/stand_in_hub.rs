//! What the pages' tests against a stand-in hub share: a window to act in, and a wait for what a page waits on, which
//! gives up within CI's limit on a test.

use std::time::Duration;

use gpui_kit::{Context, IntoElement, Render, TestAppContext, Window};

/// How long a test waits for what a stand-in hub answers at once, which leaves room under the 10 s CI fails a test
/// at.
pub(super) const SOON: Duration = Duration::from_secs(5);

/// A window that draws nothing, for a page's state to act in.
pub(super) struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::Empty
    }
}

/// Runs what the page waits on until `done`, or fails after [`SOON`].
pub(super) fn until(cx: &mut TestAppContext, what: &str, done: impl FnMut(&mut TestAppContext) -> bool) {
    crate::executor_waits::until(cx, SOON, format!("timed out waiting for {what}"), done);
}
