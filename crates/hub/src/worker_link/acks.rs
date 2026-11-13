//! Telling a worker what the hub has recorded of what it numbered, so it can let go of it. A worker keeps everything
//! numbered until a hub says so, for the next hub, should this one go first.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::model::SessionId;
use protocol::worker::Received;
use tokio::sync::{Notify, oneshot};

use super::WorkerLink;
use crate::hub::Hub;
use crate::session::Item;

/// How long the hub waits after something numbered arrives before saying it's recorded, so one acknowledgement covers
/// a burst, as when an agent writes a long reply.
const GATHERING: Duration = Duration::from_millis(100);

#[derive(Default)]
pub(crate) struct Acks {
    routed: Mutex<Routed>,
    more: Notify,
}

/// What's gone to sessions since the last acknowledgement.
#[derive(Default)]
struct Routed {
    /// The number of the last, which is the highest, since a worker numbers what it sends in order.
    through: Option<u64>,
    sessions: HashSet<SessionId>,
}

impl Acks {
    /// The frame numbered `seq` has gone to the queue of the session `session_id`.
    pub(super) fn routed(&self, session_id: &SessionId, seq: u64) {
        {
            let mut routed = self.routed.lock();
            routed.through = Some(seq);
            routed.sessions.insert(session_id.clone());
        }
        self.more.notify_one();
    }

    /// Waits for more to acknowledge, and a moment longer for the rest of a burst.
    pub(super) async fn more(&self) {
        self.more.notified().await;
        tokio::time::sleep(GATHERING).await;
    }

    /// Once each session has recorded what went to it, which is what's ahead of a flush in its queue, the number of
    /// the last that went, if anything did since it was last asked.
    pub(super) async fn recorded(&self, hub: &Arc<Hub>) -> Option<u64> {
        let Routed { through, sessions } = std::mem::take(&mut *self.routed.lock());
        let through = through?;
        for session_id in sessions {
            let (done, flushed) = oneshot::channel();
            hub.route(&session_id, Item::Flush(done)).await;
            let _ = flushed.await;
        }
        Some(through)
    }
}

/// Tells the worker what the hub has recorded, as it comes, until the connection ends.
pub(super) async fn acknowledge(hub: Arc<Hub>, link: Arc<WorkerLink>) {
    loop {
        tokio::select! {
            () = link.acks.more() => {}
            () = link.wait_closed() => return,
        }
        if let Some(through) = link.acks.recorded(&hub).await
            && link.send(Received { through }).await.is_err()
        {
            return;
        }
    }
}
