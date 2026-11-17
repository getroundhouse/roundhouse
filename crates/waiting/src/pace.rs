use std::time::Duration;

/// The longest a wait leaves between looks, so it notices what it waits for within this of its happening.
const AT_MOST: Duration = Duration::from_millis(10);

/// The longest a wait on a clock that has run ahead of the system's leaves between looks. A paused clock moves on to
/// each look at once, so a wait of minutes on it would take tens of thousands of looks, and as long to run, at
/// [`AT_MOST`]. On it, what's waited for is seen up to this late by that clock, which is as long as the longest any
/// test here left between looks before there was this crate.
const RUN_AHEAD_AT_MOST: Duration = Duration::from_millis(100);

/// How long a wait leaves before it looks again: a millisecond at first, since most of what's waited for is a few
/// milliseconds away, then twice as long each time, up to [`AT_MOST`].
pub(crate) struct Pace(Duration);

impl Pace {
    pub(crate) const fn new() -> Self {
        Self(Duration::from_millis(1))
    }

    /// How long to leave before the next look.
    pub(crate) fn next_pause(&mut self) -> Duration {
        let pause = self.0;
        self.0 = (self.0 * 2).min(AT_MOST);
        pause
    }

    /// The same, on a clock that has run `ahead` of the system's since the wait began, as tokio's paused clock does:
    /// a tenth of how far ahead it is, once that's longer, up to [`RUN_AHEAD_AT_MOST`]. On a clock that keeps to the
    /// system's, it's [`Pace::next_pause`].
    pub(crate) fn next_pause_ahead(&mut self, ahead: Duration) -> Duration {
        self.next_pause().max((ahead / 10).min(RUN_AHEAD_AT_MOST))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pace_doubles_from_a_millisecond_up_to_its_most() {
        let mut pace = Pace::new();

        let pauses: Vec<u128> = std::iter::repeat_with(|| pace.next_pause().as_millis()).take(6).collect();

        assert_eq!(pauses, [1, 2, 4, 8, 10, 10]);
    }

    #[test]
    fn a_pace_on_a_clock_run_ahead_is_a_tenth_of_how_far_up_to_its_most() {
        let mut pace = Pace::new();

        let pauses: Vec<u128> = [0, 50, 500, 5000, 60_000]
            .map(|ahead| pace.next_pause_ahead(Duration::from_millis(ahead)).as_millis())
            .to_vec();

        assert_eq!(pauses, [1, 5, 50, 100, 100]);
    }
}
