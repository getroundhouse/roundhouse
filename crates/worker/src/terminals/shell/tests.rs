//! Opening a terminal's PTY.

use std::sync::atomic::{AtomicBool, Ordering};

use portable_pty::{PtyPair, PtySize, PtySystem, native_pty_system};
use protocol::terminal::TerminalSize;

use super::command::shell;
use super::serving::{OPENING_A_PTY, open_pty};
use crate::testing::given;

/// The system's PTYs, noting whether a thread held [`OPENING_A_PTY`] as each opened.
#[derive(Default)]
struct Watched {
    alone: AtomicBool,
}

impl PtySystem for Watched {
    fn openpty(&self, size: PtySize) -> anyhow::Result<PtyPair> {
        self.alone.store(OPENING_A_PTY.is_locked(), Ordering::SeqCst);
        native_pty_system().openpty(size)
    }
}

/// macOS's `openpty` isn't safe to call from two threads at once, as [`OPENING_A_PTY`] says, and losing that race is
/// too rare to show in a test that's quick, so this shows that each holds the lock as it opens. Nothing else in these
/// tests opens a PTY, so only this one can hold it.
#[test]
fn a_pty_opens_only_while_no_other_is_opening() {
    let watched = Watched::default();

    open_pty(&watched, TerminalSize { cols: 80, rows: 24 }).unwrap();

    assert!(watched.alone.load(Ordering::SeqCst), "it opened without the lock");
}

/// The shell a test gives what it gives the worker's git has nothing of the test's own environment, such as what Cargo
/// sets for a test, but what the worker tells it, its `TERM` and the variables it's started with, and what the test
/// gives it.
#[test]
fn a_shell_given_what_a_test_gives_has_only_what_it_was_told() {
    assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some(), "Cargo sets it for a test");
    let gives = process::testing::git_environment();

    let command = shell(".", vec![("CARGO_PKG_NAME".into(), "told".into())], &given());

    let mut names: Vec<&str> = command.iter_full_env_as_str().map(|(name, _)| name).collect();
    names.sort_unstable();
    let gives = gives.vars().map(|(name, _)| name.to_str().unwrap());
    let mut expected: Vec<&str> = gives.chain(["CARGO_PKG_NAME", "COLORTERM", "TERM"]).collect();
    expected.sort_unstable();
    assert_eq!(names, expected);
}
