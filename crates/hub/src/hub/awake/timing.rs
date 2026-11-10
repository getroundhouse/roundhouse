//! When to start and stop what keeps the machine awake. It goes by the time it's given rather than the clock, so its
//! rules are tested with made-up times, without waiting for any of them.

use std::time::Duration;

use tokio::time::Instant;

use super::Hold;

#[cfg(test)]
mod tests;

/// How long each wait is.
#[derive(Clone, Copy, Debug)]
pub(super) struct Delays {
    /// How long the machine stays awake after the work stops.
    pub(super) linger: Duration,
    /// How long before the program is started again, once it couldn't start or stopped by itself.
    pub(super) retry: Duration,
}

/// What to do about the program now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Start,
    Stop,
    Nothing,
}

#[derive(Debug, Default)]
pub(super) struct Timing {
    /// When to let go, once the work has stopped and hasn't started again.
    let_go_at: Option<Instant>,
    /// Not to be started before this, after it couldn't start or stopped by itself.
    start_again_at: Option<Instant>,
}

impl Timing {
    /// What to do at `now`, given what's wanted and whether the program is `running`.
    pub(super) fn step(&mut self, wanted: Hold, running: bool, now: Instant, delays: Delays) -> Step {
        match wanted {
            Hold::Keep => {
                self.let_go_at = None;
                if running || self.start_again_at.is_some_and(|at| now < at) {
                    return Step::Nothing;
                }
                self.start_again_at = None;
                Step::Start
            }
            Hold::LetGoSoon if running => {
                let at = *self.let_go_at.get_or_insert(now + delays.linger);
                if now < at {
                    return Step::Nothing;
                }
                self.let_go_at = None;
                Step::Stop
            }
            // With nothing running there's nothing to let go of, and nothing to start again.
            Hold::LetGoSoon => {
                *self = Self::default();
                Step::Nothing
            }
            Hold::LetGoNow => {
                *self = Self::default();
                if running { Step::Stop } else { Step::Nothing }
            }
        }
    }

    /// The program couldn't start, or stopped by itself, at `now`. It isn't started again straight away, so one that
    /// can't run costs a start every so often rather than one after another without end.
    pub(super) fn failed(&mut self, now: Instant, delays: Delays) {
        self.start_again_at = Some(now + delays.retry);
    }

    /// When to take the next step if nothing has changed by then, or `None` if only a change brings one.
    pub(super) fn next_step_at(&self) -> Option<Instant> {
        match (self.let_go_at, self.start_again_at) {
            (Some(let_go), Some(start_again)) => Some(let_go.min(start_again)),
            (at, None) | (None, at) => at,
        }
    }
}
