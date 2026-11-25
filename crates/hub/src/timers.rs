//! The hub's timers that a test of the hub from outside it would otherwise sit through, or time by the clock, gathered so
//! a test's hub can be given shorter ones, or, for a test that something didn't wait for a time limit, a longer one.
//! The hub's own tests pause tokio's clock instead, which checks each timer's rules without waiting.

#[cfg(any(test, feature = "testing"))]
use protocol::testing::ENV_TEST_HUB_TIMERS;
use std::time::Duration;

/// How long the hub waits by the clock, where a test of it from outside would otherwise wait as long. A hub runs with
/// [`Timers::DEFAULT`], and only a test's daemon gives it others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "testing"), derive(serde::Serialize, serde::Deserialize))]
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
    /// How long a provider's adapter has to answer the hub's hello, which is the provider protocol's
    /// `HELLO_ANSWERED_WITHIN`.
    pub adapter_hello_within: Duration,
    /// How long the hub waits for a provider as it starts, before it serves, to list its instances or say how to reach
    /// a worker an earlier hub left running there. One that takes longer has its sessions interrupted, as if their
    /// workers had gone, so a slow one doesn't keep every client waiting. For an adapter, it's the provider protocol's
    /// `ANSWERED_AS_THE_HUB_STARTS_WITHIN`.
    pub before_serving: Duration,
    /// How often the hub looks at a workspace whose worker is another build.
    pub look_every: Duration,
    /// How often the hub looks for a pull request to read that nobody asked for.
    pub read_pace: Duration,
    /// How long after a read of a workspace's open pull request began a client that asks for it takes what that read
    /// found, rather than having it read again.
    pub fresh_for_clients: Duration,
    /// How long after the hub tried to read a pull request its cache had forgotten, of an archived workspace a client
    /// shows, it tries again, where the read failed or found nothing.
    pub read_forgotten_again: Duration,
    /// The least time between two asks after a sign-in's token, whatever GitHub says, so a code that says none between
    /// them isn't asked after in a loop.
    pub sign_in_pace: Duration,
    /// How often the hub checks its GitHub logins by the wall clock, after the check as it starts: whether the default's
    /// token has ended, and which sign-ins to renew and installations to make a token for, as
    /// `Hub::watch_github_logins` says.
    pub check_github_logins_every: Duration,
    /// How often the hub checks its sign-ins to tool servers by the wall clock, for which to renew, after the check as it
    /// starts.
    pub check_tool_sign_ins_every: Duration,
    /// How often a hub installed as a service reads the update feed, after the read as it starts.
    pub look_for_updates_every: Duration,
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
        adapter_hello_within: protocol::provider::HELLO_ANSWERED_WITHIN,
        // The local provider answers in milliseconds, so this is reached by a provider that's stuck, or slow enough
        // that waiting for it would keep every client waiting.
        before_serving: protocol::provider::ANSWERED_AS_THE_HUB_STARTS_WITHIN,
        // Two looks in a row have to find nothing at work, so a moment with nothing at work, as just before an agent
        // picks up again by itself once its work in the background is done, isn't taken for the workspace being idle.
        look_every: Duration::from_secs(5),
        // A read in turn is due minutes apart, so looking more often finds nothing new, and costs only a look at the
        // store.
        read_pace: Duration::from_secs(1),
        // Long enough for what follows a read closely, such as a pane shown as its turn ends, or a second client
        // showing it, and short enough that a pull request that's still moving looks no staler than the 30 seconds a
        // client asks again in. *Unconfirmed.*
        fresh_for_clients: Duration::from_secs(10),
        // As a chip's read that failed is tried again, so a GitHub that's down or slow is asked no more often for these
        // than for those. *Unconfirmed.*
        read_forgotten_again: Duration::from_secs(60),
        // GitHub's docs give five seconds between asks, so only a code that says less than this keeps to it.
        sign_in_pace: Duration::from_secs(1),
        // Tokio's timers don't count the time a Mac sleeps, so waiting until a token's end, or until it's due to be
        // renewed, would wait as long again as the Mac slept, where checking every minute it's awake finds it within a
        // minute of waking. Until then, a push fails at GitHub, which says the token is bad, rather than with the
        // block, which says what to do. *Unconfirmed:* a minute.
        check_github_logins_every: Duration::from_secs(60),
        // As its GitHub logins are, for the same reason.
        check_tool_sign_ins_every: Duration::from_secs(60),
        // As often as the app looks for its own.
        look_for_updates_every: Duration::from_mins(10),
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
        var(ENV_TEST_HUB_TIMERS)
            .map_or(Self::DEFAULT, |given| serde_json::from_str(&given).expect("timers as `Timers::env` writes them"))
    }

    /// What a test's hub runs with, unless the test is about one of them: the timers that only make a test wait
    /// shortened, as is how long a client's ask takes what a read just before found, which a test would otherwise wait
    /// out to see a change, and the time limits as they are, since they guard against a process being slow, which one
    /// on a busy runner can be. A test about a time limit sets it to a second or a few, which still outlasts a process
    /// starting there. The GitHub logins are checked as seldom as the product checks them, since a check can call
    /// GitHub, whose stand-in gives a test's scripted answers in turn, so a check the test didn't ask for would take
    /// one. A test about a check after the first sets how often.
    pub const FOR_TESTS: Self = Self {
        agent_spacing: Duration::from_millis(100),
        look_every: Duration::from_millis(100),
        read_pace: Duration::from_millis(100),
        fresh_for_clients: Duration::ZERO,
        read_forgotten_again: Duration::from_millis(100),
        sign_in_pace: Duration::from_millis(100),
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
        fn a_hub_takes_the_timers_a_test_gives_it(times in prop::array::uniform14(duration())) {
            let [
                agent_spacing,
                picks_up_within,
                shutdown_grace,
                hello_within,
                adapter_hello_within,
                before_serving,
                look_every,
                read_pace,
                fresh_for_clients,
                read_forgotten_again,
                sign_in_pace,
                check_github_logins_every,
                check_tool_sign_ins_every,
                look_for_updates_every,
            ] = times;
            let given = Timers {
                agent_spacing,
                picks_up_within,
                shutdown_grace,
                hello_within,
                adapter_hello_within,
                before_serving,
                look_every,
                read_pace,
                fresh_for_clients,
                read_forgotten_again,
                sign_in_pace,
                check_github_logins_every,
                check_tool_sign_ins_every,
                look_for_updates_every,
            };
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
