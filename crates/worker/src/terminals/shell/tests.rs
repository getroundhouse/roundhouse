//! Opening a terminal's PTY.

use std::sync::atomic::{AtomicBool, Ordering};

use portable_pty::{PtyPair, PtySize, PtySystem, native_pty_system};
use protocol::terminal::TerminalSize;

use super::serving::{OPENING_A_PTY, open_pty};

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
