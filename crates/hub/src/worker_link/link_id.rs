use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// Which of this hub's connections to a worker a [`WorkerLink`](super::WorkerLink) is, so what ran on an earlier one to
/// the same workspace, such as an agent or a terminal's shell, isn't taken for what runs on the current one. In memory
/// only, from 1 as the hub starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkerLinkId(u64);

impl WorkerLinkId {
    /// One no link of this hub's has had.
    pub(super) fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

impl fmt::Display for WorkerLinkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
