//! What the app says when someone checks for updates.

use gpui_kit::{App, PromptLevel};
use velopack::UpdateInfo;

use super::Problem;

/// What a check found.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Answer {
    /// Nothing is newer than this version.
    UpToDate { version: String },
    /// This version is downloaded, and installs when the app restarts to update or quits.
    Ready { version: String },
    /// What stopped the check, or stops this app updating at all.
    Failed(Problem),
    /// A build Velopack didn't install, which never updates.
    NotUpdating,
}

impl Answer {
    /// What the look that answers someone found. An update an earlier look downloaded counts, since it's what
    /// installs. Any problem is said at once, even being out of reach, which the sidebar waits a day to show, because
    /// someone asked.
    pub(super) fn after(outcome: &Result<(), Problem>, ready: Option<&UpdateInfo>, current: String) -> Self {
        match (outcome, ready) {
            (Err(problem), _) => Self::Failed(problem.clone()),
            (Ok(()), Some(info)) => Self::Ready { version: info.TargetFullRelease.Version.clone() },
            (Ok(()), None) => Self::UpToDate { version: current },
        }
    }

    pub(super) fn show(self, cx: &mut App) {
        let (level, message, detail) = match self {
            Self::UpToDate { version } => {
                (PromptLevel::Info, "Roundhouse is up to date".to_string(), format!("{version} is the newest version."))
            }
            Self::Ready { version } => {
                let detail = "Restart to update, at the bottom of the sidebar, installs it now. Otherwise it installs when \
                              you quit.";
                (PromptLevel::Info, format!("Roundhouse {version} is ready"), detail.to_string())
            }
            Self::Failed(problem) => (PromptLevel::Warning, problem.summary(), problem.detail),
            Self::NotUpdating => (
                PromptLevel::Info,
                "This build doesn't update itself".to_string(),
                "Only Roundhouse installed from a release updates itself.".to_string(),
            ),
        };
        crate::alert::show(level, message, detail, cx);
    }
}
