//! The daemons these tests start: building them, and the environment they run in.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use process::testing::WholeEnvironment;

/// The binaries live in other packages, so `cargo test -p desktop` wouldn't otherwise rebuild them.
pub(crate) fn binaries() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let tmp = Path::new(env!("CARGO_TARGET_TMPDIR"));
        process::testing::the_examples(tmp, "ui-binaries", &["daemon_for_tests", "fake_claude", "stand_in_hub"])
    })
}

/// The whole environment of every daemon these tests start, whether a test starts it or the app does: what
/// [`WholeEnvironment::for_a_test`] gives, with a home of its own, so a real ~/.conductor/settings.toml or `gh` login
/// can't change what these tests do, a `gh` that's logged in to nothing, the fake agent, and the timers that only make
/// a test wait shortened. A hub a client starts is told where its data and workspaces are as it starts.
pub(crate) fn hub_environment() -> WholeEnvironment {
    // Under Cargo's target directory, so runs can't pile up in the system's. Conversation IDs are unique, so nothing
    // here carries over from one run to the next.
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let (timers, short) = hub::Timers::FOR_TESTS.env();
    let (worker_timers, worker_short) = worker::Timers::FOR_TESTS.env();
    let mut environment = WholeEnvironment::for_a_test(&scratch.join("ui-home"));
    environment
        // Ahead of the person's own. A test that has `gh` answer puts its own ahead of this one.
        .put_first_on_path(process::testing::logged_out_gh())
        .set("ROUNDHOUSE_CLAUDE_BIN", binaries().join("fake_claude"))
        // So a test waits out only the time limits it's about. The hub passes the workers' on to them.
        .set(timers, short)
        .set(worker_timers, worker_short)
        .set("FAKE_CLAUDE_STATE", scratch.join("ui-agent-state"))
        // So an agent started again doesn't sit through the CLI's 2 s before it asks after a task.
        .set("FAKE_CLAUDE_POLL_DEFAULT_MS", worker::Timers::FOR_TESTS.wait_poll.as_millis().to_string());
    environment
}
