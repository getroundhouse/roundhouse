//! The hubs a test's daemon starts, which the test stops as it ends.

use std::io;
use std::process::Child;
use std::sync::Arc;

use parking_lot::Mutex;

/// The hubs a test's [`Daemon`](crate::Daemon) has started, whoever asked it to, such as the app the test drives,
/// which connects again, starting a hub, as soon as the test stops the one it has. Once the test has closed it, the
/// daemon starts no more, so none starts after the test has ended, and the test stops those it gives back. Every clone
/// is the same record.
#[derive(Clone, Default)]
pub struct StartedHubs(Arc<Mutex<Started>>);

#[derive(Default)]
struct Started {
    closed: bool,
    hubs: Vec<Child>,
}

impl StartedHubs {
    /// Starts no more, and gives back each hub it started, which may have exited already.
    pub fn close(&self) -> Vec<Child> {
        let mut started = self.0.lock();
        started.closed = true;
        std::mem::take(&mut started.hubs)
    }

    /// Starts a hub with `start`, and keeps it, unless it's closed, when it doesn't call `start` at all. It holds the
    /// lock while `start` makes the hub's directory and starts it, which is the point: closing waits until this is
    /// done, so a hub started as it closes is among those it gives back, and none starts after.
    pub(super) fn start(&self, start: impl FnOnce() -> io::Result<Child>) -> io::Result<()> {
        let mut started = self.0.lock();
        if started.closed {
            return Err(io::Error::other("the test this daemon is for has ended, so it starts no hub"));
        }
        started.hubs.push(start()?);
        Ok(())
    }
}

/// Every clone is the same record, so two are equal only if they're clones.
impl PartialEq for StartedHubs {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for StartedHubs {}

impl std::fmt::Debug for StartedHubs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let started = self.0.lock();
        f.debug_struct("StartedHubs").field("closed", &started.closed).field("hubs", &started.hubs.len()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn it_keeps_each_hub_it_starts_and_starts_none_once_closed() {
        let started = StartedHubs::default();
        let clone = started.clone();
        clone.start(|| Command::new("true").spawn()).unwrap();

        let mut kept = started.close();

        assert_eq!(kept.len(), 1, "a clone records into the same");
        assert!(kept[0].wait().unwrap().success());
        let mut asked = false;
        let refused = clone.start(|| {
            asked = true;
            Command::new("true").spawn()
        });
        assert!(refused.is_err());
        assert!(!asked, "it started nothing");
        assert!(started.close().is_empty());
    }

    #[test]
    fn a_hub_that_fails_to_start_isnt_kept() {
        let started = StartedHubs::default();

        let failed = started.start(|| Command::new("/nonexistent/roundhoused").spawn());

        assert!(failed.is_err());
        assert!(started.close().is_empty());
    }
}
