//! How the app's own tests wait for what the app does on the system's clock, such as a real hub or server answering.

use std::fmt::Display;
use std::time::Duration;

use gpui_kit::TestAppContext;

/// Returns once `done` holds, running what the app has to do each time it doesn't, and panics with `failure` once
/// `limit` has passed without it, as [`waiting::thread::until`] does.
#[track_caller]
pub(crate) fn until(
    cx: &mut TestAppContext,
    limit: Duration,
    failure: impl Display,
    mut done: impl FnMut(&mut TestAppContext) -> bool,
) {
    waiting::thread::until(limit, failure, || {
        done(cx) || {
            cx.run_until_parked();
            false
        }
    });
}
