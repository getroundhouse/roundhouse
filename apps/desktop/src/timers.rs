//! How long the app waits by the system's clock, which a test can't move as it does GPUI's, gathered so a test can be
//! given shorter ones, as `hub::Timers` are for a test's hub.

use std::time::Duration;

use gpui_kit::{App, Global};

/// The app as it runs has [`Timers::DEFAULT`], and only a test gives it others.
#[derive(Clone, Copy, Debug)]
pub struct Timers {
    /// The longest quitting, or restarting to update, waits for what's on its way to the hub, such as a file attached
    /// just before, before asking about whatever it would still lose.
    pub keeping: Duration,
    /// How long the app waits for GitHub to send the person back once its page has opened.
    pub github_sends_back: Duration,
}

impl Global for Timers {}

impl Timers {
    /// What the app runs with.
    pub const DEFAULT: Self = Self {
        // GPUI gives the app's own wait as it quits a tenth of this. *Unconfirmed.*
        keeping: Duration::from_secs(2),
        // Time to sign in to GitHub and look over the form, but not so long that a page left open stays a way in.
        // *Unconfirmed.*
        github_sends_back: Duration::from_mins(30),
    };

    /// Those the app was given.
    pub(crate) fn of(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or(Self::DEFAULT)
    }
}
