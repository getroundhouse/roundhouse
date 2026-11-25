//! What the sidebar says when someone checks for updates and nothing else there answers them. A downloaded update
//! answers as its card, and a problem as itself.

use std::time::Duration;

/// How long an answer stays in the version's place. Long enough to read after looking down from the menu, and short
/// enough that it doesn't become something else to read in the sidebar.
pub(super) const SHOWN_FOR: Duration = Duration::from_secs(5);

/// What a check found, when it isn't an update or a problem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    /// Nothing is newer than this version, and nothing newer is downloaded.
    UpToDate,
    /// A build Velopack didn't install, which never updates.
    NotUpdating,
}

impl Answer {
    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::UpToDate => "Up to date",
            Self::NotUpdating => "This build doesn't update itself",
        }
    }
}

/// How the app's own update stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnUpdate<'a> {
    /// Downloaded, as this version.
    Ready(&'a str),
    /// Someone checked, and the look that answers them hasn't finished.
    Checking,
    /// The last look that worked found nothing newer.
    NothingNewer,
    /// No look has worked yet.
    NotFound,
    /// This build doesn't update itself, or can't where it is.
    Never,
}
