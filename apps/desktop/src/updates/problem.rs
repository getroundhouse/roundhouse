//! What goes wrong updating, what it means for the person using the app, and when the sidebar says so.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui_kit::Global;

use super::source::SourceError;

/// How long the update server can be out of reach before the sidebar says so. A laptop is often offline for an hour or
/// two, which says nothing about updating.
const QUIET_FOR: Duration = Duration::from_hours(24);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Check,
    Download,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The update server couldn't be reached, or didn't send the file, for longer than [`QUIET_FOR`].
    Unreachable,
    /// The feed or a package wasn't what the release workflow signed, so nothing from it was used.
    Refused(Refusal),
    /// Anything else that stopped a check or a download.
    Failed(Step),
    /// macOS runs the app from a read-only copy, because it was opened where it was downloaded.
    Translocated,
    /// The app is somewhere this user can't change.
    ReadOnly,
}

/// What wasn't as the release workflow signed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The update feed's signature doesn't match it.
    Unsigned,
    /// A download is longer than the update feed says.
    TooLong,
    /// A package isn't the one the update feed lists.
    NotAsListed,
}

/// Why the app can't update. The sidebar says so until a later look succeeds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub kind: Kind,
    /// Velopack's, the source's or the system's own words, for whoever wants the detail.
    pub detail: String,
}

impl Problem {
    pub fn summary(&self) -> String {
        let refused = |what: &str| format!("Update refused: {what}");
        match self.kind {
            Kind::Unreachable => "Couldn't reach the update server".to_string(),
            Kind::Refused(Refusal::Unsigned) => refused("the update feed's signature doesn't match"),
            Kind::Refused(Refusal::TooLong) => refused("a download is longer than it should be"),
            Kind::Refused(Refusal::NotAsListed) => refused("a download doesn't match the update feed"),
            Kind::Failed(Step::Check) => "Couldn't check for updates".to_string(),
            Kind::Failed(Step::Download) => "Couldn't download an update".to_string(),
            Kind::Translocated => "Move Roundhouse to Applications to update it".to_string(),
            Kind::ReadOnly => "Can't update Roundhouse where it is".to_string(),
        }
    }
}

/// What a failure means for the person using the app. The source's own errors come back from Velopack whole.
pub(super) fn problem(step: Step, error: &velopack::Error) -> Problem {
    let source = match error {
        velopack::Error::Io(io) => io.get_ref().and_then(|inner| inner.downcast_ref::<SourceError>()),
        _ => None,
    };
    let kind = match (source, error) {
        (
            Some(
                SourceError::Fetch { .. }
                | SourceError::Status { .. }
                | SourceError::Read { .. }
                | SourceError::CutShort { .. },
            ),
            _,
        )
        | (None, velopack::Error::Network(_)) => Kind::Unreachable,
        (Some(SourceError::Unsigned), _) => Kind::Refused(Refusal::Unsigned),
        (Some(SourceError::TooLong { .. }), _) => Kind::Refused(Refusal::TooLong),
        (None, velopack::Error::ChecksumInvalid(..) | velopack::Error::SizeInvalid(..)) => {
            Kind::Refused(Refusal::NotAsListed)
        }
        (Some(SourceError::Unreadable(_) | SourceError::Save { .. }) | None, _) => Kind::Failed(step),
    };
    Problem { kind, detail: source.map_or_else(|| error.to_string(), ToString::to_string) }
}

/// What the sidebar shows about updating.
#[derive(Default)]
pub(super) struct Shown {
    pub(super) problem: Option<Problem>,
    /// Someone checked for updates, and the look that answers them hasn't finished.
    pub(super) checking: bool,
}

impl Global for Shown {}

impl Shown {
    /// Takes in how a look went, and how long the update server has been out of reach. Being out of reach only shows
    /// once it has lasted, and anything else at once. Whoever checked has their answer.
    pub(super) fn record(&mut self, outcome: Result<(), Problem>, out_of_reach: Duration) {
        self.checking = false;
        match outcome {
            Ok(()) => self.problem = None,
            Err(problem) if problem.kind == Kind::Unreachable && out_of_reach < QUIET_FOR => {}
            Err(problem) => self.problem = Some(problem),
        }
    }
}

/// When the update server went out of reach, if no look has reached it since. It's kept in a file, so it counts across
/// restarts, and in the time on the clock, which goes on while the Mac sleeps.
pub(super) struct OutOfReach {
    file: PathBuf,
}

impl OutOfReach {
    pub(super) fn in_dir(dir: &Path) -> Self {
        Self { file: dir.join("update-server-out-of-reach-since") }
    }

    /// Takes in whether a look reached the update server, and returns how long it has been out of reach.
    pub(super) fn note(&self, reached: bool, now: SystemTime) -> Duration {
        if reached {
            // Most looks reach it, so there's usually nothing to remove.
            let _ = std::fs::remove_file(&self.file);
            return Duration::ZERO;
        }
        if let Some(since) = self.since() {
            // A clock set back counts as no time.
            return now.duration_since(since).unwrap_or_default();
        }
        // If this can't be saved, such as before the hub has made the directory, the next look starts the count.
        let seconds = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let _ = std::fs::write(&self.file, seconds.to_string());
        Duration::ZERO
    }

    fn since(&self) -> Option<SystemTime> {
        let seconds = std::fs::read_to_string(&self.file).ok()?.trim().parse().ok()?;
        Some(UNIX_EPOCH + Duration::from_secs(seconds))
    }
}

#[cfg(test)]
mod tests;
