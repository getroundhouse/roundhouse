//! The hub's timers that a test of the hub from outside it would otherwise sit through, gathered so a test's hub can be
//! given shorter ones. The hub's own tests pause tokio's clock instead, which checks each timer's rules without waiting.

#[cfg(feature = "testing")]
use protocol::paths::ENV_TEST_HUB_TIMERS;
use std::time::Duration;

/// How long the hub waits by the clock, where a test of it from outside would otherwise wait as long. A hub runs with
/// [`Timers::DEFAULT`], and only a test's takes others, as [`Timers::from_env`] says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "testing", derive(serde::Serialize, serde::Deserialize))]
pub struct Timers {
    /// How long after a workspace's last read of its pull request an agent asking waits before its own.
    pub agent_spacing: Duration,
    /// How long the agent has to pick up again by itself once the last of its work in the background has gone between
    /// turns, before the hub takes it to be idle.
    pub picks_up_within: Duration,
    /// How long a worker the hub has asked to stop has, before the hub ends it as stuck. As the hub restarts, it's how
    /// long its workers have to let it go, before it goes all the same.
    pub shutdown_grace: Duration,
    /// How long a worker has to answer the hub's hello.
    pub hello_within: Duration,
    /// How often the hub looks at a workspace whose worker is another build.
    pub look_every: Duration,
    /// How often the hub looks for a pull request to read that nobody asked for, while the budget can spare the calls.
    pub read_pace: Duration,
}

impl Timers {
    /// What a hub runs with.
    pub const DEFAULT: Self = Self {
        // Long enough that an agent asking in a loop reads at most six times a minute, and short enough that one
        // asking again after it pushes waits little.
        agent_spacing: Duration::from_secs(10),
        // The CLI does within a tenth of a second. An agent asked to stop says its work was stopped too, and exits
        // within eight seconds, three before it's killed and five for its last output, so its exit comes first, and
        // says it was cut short.
        picks_up_within: Duration::from_secs(15),
        // Agents get 3 s to stop, then up to 5 s for each of their two streams to end, which something an agent left
        // running can hold open, and the worker exits within a second of flushing after that. So this is only reached
        // if something is stuck.
        shutdown_grace: Duration::from_secs(15),
        // A worker on this machine answers in milliseconds. Hitting this means something is wrong with it.
        hello_within: Duration::from_secs(10),
        // Two looks in a row have to find nothing at work, so a moment with nothing at work, as just before an agent
        // picks up again by itself once its work in the background is done, isn't taken for the workspace being idle.
        look_every: Duration::from_secs(5),
        // The budget's steady rate is a call a second, so looking more often would find nothing it could spare.
        read_pace: Duration::from_secs(1),
    };

    /// [`Timers::DEFAULT`], whatever `var` says, since only a build with the `testing` feature takes others, and the
    /// product is never built with it.
    #[cfg(not(feature = "testing"))]
    pub fn from_env(_var: impl Fn(&str) -> Option<String>) -> Self {
        Self::DEFAULT
    }

    /// What `var` gives for the variable [`Timers::env`] names, as it wrote them, and otherwise [`Timers::DEFAULT`].
    ///
    /// # Panics
    ///
    /// On a value [`Timers::env`] didn't write, which only a mistake in a test gives it.
    #[cfg(feature = "testing")]
    #[expect(
        clippy::expect_used,
        reason = "only a test sets it, through `Timers::env`, so one that doesn't parse is a mistake in the test"
    )]
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Self {
        var(ENV_TEST_HUB_TIMERS)
            .map_or(Self::DEFAULT, |given| serde_json::from_str(&given).expect("timers as `Timers::env` writes them"))
    }
}

#[cfg(feature = "testing")]
impl Timers {
    /// What a test's hub runs with, unless the test is about one of them: the timers that only make a test wait
    /// shortened, and the time limits as they are, since they guard against a process being slow, which one on a busy
    /// runner can be. A test about a time limit sets it to a second or a few, which still outlasts a process
    /// starting there.
    pub const FOR_TESTS: Self = Self {
        agent_spacing: Duration::from_millis(100),
        look_every: Duration::from_millis(100),
        read_pace: Duration::from_millis(100),
        ..Self::DEFAULT
    };

    /// The variable to give a hub, and its value, to have it run with these.
    #[must_use]
    #[expect(clippy::expect_used, reason = "nothing but durations, which always serialize")]
    pub fn env(&self) -> (&'static str, String) {
        (ENV_TEST_HUB_TIMERS, serde_json::to_string(self).expect("durations serialize"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    fn duration() -> impl Strategy<Value = Duration> {
        (any::<u64>(), 0..1_000_000_000u32).prop_map(|(secs, nanos)| Duration::new(secs, nanos))
    }

    proptest! {
        #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

        #[test]
        fn a_hub_takes_the_timers_a_test_gives_it(times in prop::array::uniform6(duration())) {
            let [agent_spacing, picks_up_within, shutdown_grace, hello_within, look_every, read_pace] = times;
            let given = Timers { agent_spacing, picks_up_within, shutdown_grace, hello_within, look_every, read_pace };
            let (name, value) = given.env();

            let taken = Timers::from_env(|asked| (asked == name).then(|| value.clone()));

            prop_assert_eq!(taken, given);
        }
    }

    #[test]
    fn a_hub_given_none_runs_with_the_defaults() {
        assert_eq!(Timers::from_env(|_| None), Timers::DEFAULT);
    }
}
