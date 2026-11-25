//! The worker's timers that a test of it from outside it would otherwise sit through, gathered so a test's worker can
//! be given shorter ones, as `hub::Timers` are for a test's hub. The worker's own tests check each timer's rules
//! without waiting.

#[cfg(any(test, feature = "testing"))]
use protocol::testing::ENV_TEST_WORKER_TIMERS;
use std::time::Duration;

/// How long the worker has others wait by the clock, where a test of it from outside would otherwise wait as long. A
/// worker runs with [`Timers::DEFAULT`], and only a test's daemon gives it others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "testing"), derive(serde::Serialize, serde::Deserialize))]
pub struct Timers {
    /// How often the agent's CLI is asked to check on a wait.
    pub wait_poll: Duration,
    /// How long a terminal's shell asked to end, and the job it runs, have to end by themselves before they're killed.
    pub terminal_stop_grace: Duration,
    /// How often a terminal looks at which job its shell runs in front, to record it.
    pub look_in_front_every: Duration,
    /// How long an agent that has exited has to let go of its output, as `agents::DrainGrace` says.
    pub agent_drain: Duration,
    /// How long the hub connected has to go once another connects, before the other is refused.
    pub hub_goes_within: Duration,
}

impl Timers {
    /// What a worker runs with.
    pub const DEFAULT: Self = Self {
        // The hub reads the pull request less often than this, so it's only how soon after the read that ends a wait
        // the agent hears of it, at the cost of a question to the hub that never reads GitHub. *Unconfirmed*
        wait_poll: Duration::from_secs(5),
        // Long enough for a program that saves as it's hung up on, such as an editor writing its recovery file, and
        // short enough that closing a tab, or archiving a workspace, doesn't seem to hang.
        terminal_stop_grace: Duration::from_secs(2),
        // Asking is one system call, and recording one a small file, written only once another is in front, so a job
        // is recorded within a second of starting, and what it's left unrecorded for, if the worker is killed that
        // soon, is that second.
        look_in_front_every: Duration::from_secs(1),
        agent_drain: agents::DrainGrace::DEFAULT.0,
        // A crash is noticed within milliseconds, and more on a busy machine. Well under the ten seconds a hub waits for
        // an answer to its hello, since a hub starting a worker leaves one that refuses it, and ends one that doesn't
        // answer. *Unconfirmed:* five seconds.
        hub_goes_within: Duration::from_secs(5),
    };
}

#[cfg(any(test, feature = "testing"))]
impl Timers {
    /// What `var` gives for the variable [`Timers::env`] names, as it wrote them, and otherwise [`Timers::DEFAULT`].
    ///
    /// # Panics
    ///
    /// On a value [`Timers::env`] didn't write, which only a mistake in a test gives it.
    #[expect(
        clippy::expect_used,
        reason = "only a test sets it, through `Timers::env`, so one that doesn't parse is a mistake in the test"
    )]
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Self {
        var(ENV_TEST_WORKER_TIMERS)
            .map_or(Self::DEFAULT, |given| serde_json::from_str(&given).expect("timers as `Timers::env` writes them"))
    }

    /// What a test's worker runs with: a wait asked after every quarter of a second, the most often the Claude CLI
    /// takes, and so the fake agent too, and a terminal's job in front looked at twenty times a second. A terminal's
    /// stop grace is the worker's own, since it's a limit, which guards against a process being slow, as one on a busy
    /// runner can be. Only a test of it shortens it.
    pub const FOR_TESTS: Self =
        Self { wait_poll: Duration::from_millis(250), look_in_front_every: Duration::from_millis(50), ..Self::DEFAULT };

    /// The variable to give a worker, and its value, to have it run with these. A hub passes it on to the workers it
    /// starts.
    #[must_use]
    #[expect(clippy::expect_used, reason = "nothing but durations, which always serialize")]
    pub fn env(&self) -> (&'static str, String) {
        (ENV_TEST_WORKER_TIMERS, serde_json::to_string(self).expect("durations serialize"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    proptest! {
        #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

        #[test]
        fn a_worker_takes_the_timers_a_test_gives_it(
            (secs, nanos) in (any::<u64>(), 0..1_000_000_000u32),
            (grace_secs, grace_nanos) in (any::<u64>(), 0..1_000_000_000u32),
            (look_secs, look_nanos) in (any::<u64>(), 0..1_000_000_000u32),
            (drain_secs, drain_nanos) in (any::<u64>(), 0..1_000_000_000u32),
            (goes_secs, goes_nanos) in (any::<u64>(), 0..1_000_000_000u32),
        ) {
            let given = Timers {
                wait_poll: Duration::new(secs, nanos),
                terminal_stop_grace: Duration::new(grace_secs, grace_nanos),
                look_in_front_every: Duration::new(look_secs, look_nanos),
                agent_drain: Duration::new(drain_secs, drain_nanos),
                hub_goes_within: Duration::new(goes_secs, goes_nanos),
            };
            let (name, value) = given.env();

            let taken = Timers::from_env(|asked| (asked == name).then(|| value.clone()));

            prop_assert_eq!(taken, given);
        }
    }

    #[test]
    fn a_worker_given_none_runs_with_the_defaults() {
        assert_eq!(Timers::from_env(|_| None), Timers::DEFAULT);
    }
}
