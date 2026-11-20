//! The hub's counts of its clients and of the requests under way, which its status, and whether it's idle, are read
//! from.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::hub::Hub;

/// Counts one among one of the hub's counts for as long as it's held, so the count comes down however what it counts
/// ends: finished, dropped where it waited, such as a connection's read loop as its device is revoked, or unwinding
/// from a panic.
pub(super) struct Counted {
    hub: Arc<Hub>,
    count: fn(&Hub) -> &AtomicU32,
}

impl Counted {
    /// A client: a connection admitted to the hub.
    pub(super) fn client(hub: &Arc<Hub>) -> Counted {
        Counted::new(hub, |hub| &hub.clients)
    }

    /// A request, from when it's read until its reply is queued.
    pub(super) fn request(hub: &Arc<Hub>) -> Counted {
        Counted::new(hub, |hub| &hub.requests)
    }

    fn new(hub: &Arc<Hub>, count: fn(&Hub) -> &AtomicU32) -> Counted {
        count(hub).fetch_add(1, Ordering::Relaxed);
        Counted { hub: hub.clone(), count }
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        (self.count)(&self.hub).fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::Counted;
    use crate::hub::tests::hub_with_worktree;

    #[tokio::test]
    async fn a_count_comes_down_however_what_it_counts_ends() {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_with_worktree(dir.path());
        let counts = || (hub.clients.load(Ordering::Relaxed), hub.requests.load(Ordering::Relaxed));
        let (client, finished) = (Counted::client(&hub), Counted::request(&hub));
        let waiting = Counted::request(&hub);
        let waiting = tokio::spawn(async move {
            let _waiting = waiting;
            std::future::pending::<()>().await;
        });
        let panicking = Counted::request(&hub);
        let panicking = tokio::spawn(async move {
            let _panicking = panicking;
            panic!("as a request's task could");
        });
        assert_eq!(counts(), (1, 3));

        drop((client, finished));
        waiting.abort();
        assert!(waiting.await.unwrap_err().is_cancelled());
        assert!(panicking.await.unwrap_err().is_panic());

        assert_eq!(counts(), (0, 0));
    }
}
