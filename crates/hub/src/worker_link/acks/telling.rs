use std::collections::HashSet;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::ApiError;
use protocol::model::SessionId;
use protocol::worker::Received;
use tokio::sync::{Notify, oneshot};

use crate::hub::Hub;
use crate::session::Item;
use crate::worker_link::WorkerLink;

#[derive(Default)]
pub(crate) struct Acks {
    pub(super) routed: Mutex<Routed>,
    more: Notify,
    /// The number of the last frame the worker was told of, if any. Held from taking what's routed until the worker
    /// has been told of it, so a hub that leaves meanwhile waits for that, rather than finding nothing left to take
    /// and closing before it's sent.
    told: tokio::sync::Mutex<Option<u64>>,
}

/// What's gone to sessions since the last acknowledgement.
#[derive(Default)]
pub(super) struct Routed {
    /// The number of the last, which is the highest, since a worker numbers what it sends in order.
    pub(super) through: Option<u64>,
    sessions: HashSet<SessionId>,
}

impl Acks {
    /// The frame numbered `seq` has gone to the queue of the session `session_id`.
    pub(in super::super) fn routed(&self, session_id: &SessionId, seq: u64) {
        {
            let mut routed = self.routed.lock();
            routed.through = Some(seq);
            routed.sessions.insert(session_id.clone());
        }
        self.more.notify_one();
    }

    /// Waits for more to acknowledge. What arrives while the last is acknowledged leaves one wake, which waits for the
    /// next look, so one acknowledgement covers all of it, and a burst gets few without waiting for the rest of it.
    async fn more(&self) {
        self.more.notified().await;
    }

    /// Tells the worker what's recorded of what went since it was last told, if anything did, without waiting for it to
    /// let go of it.
    async fn tell(&self, hub: &Arc<Hub>, link: &WorkerLink) -> Result<(), ApiError> {
        let mut told = self.told.lock().await;
        if let Some(through) = self.recorded(hub, &mut told).await {
            drop(link.send(Received { through }).await?);
            *told = Some(through);
        }
        Ok(())
    }

    /// Tells the worker what's recorded, as [`Acks::tell`] does, and waits until it has let go of everything it's been
    /// told of, so the next hub isn't sent it again. As the hub leaves the worker, which is why it waits: the worker
    /// lets go on a task of its own, which nothing else orders before the next hub's hello.
    pub(in super::super) async fn tell_and_wait(&self, hub: &Arc<Hub>, link: &WorkerLink) -> Result<(), ApiError> {
        let mut told = self.told.lock().await;
        if let Some(through) = self.recorded(hub, &mut told).await {
            *told = Some(through);
        }
        if let Some(through) = *told {
            link.call(Received { through }).await?;
        }
        Ok(())
    }

    /// Once each session has recorded what went to it, which is what's ahead of a flush in its queue, the number of
    /// the last that went, if anything did since it was last asked. It takes what `told` holds, so it's only asked
    /// under that lock, and what it takes is told of before anyone else looks.
    async fn recorded(&self, hub: &Arc<Hub>, _told: &mut Option<u64>) -> Option<u64> {
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
pub(in super::super) async fn acknowledge(hub: Arc<Hub>, link: Arc<WorkerLink>) {
    loop {
        tokio::select! {
            () = link.acks.more() => {}
            () = link.wait_closed() => return,
        }
        if link.acks.tell(&hub, &link).await.is_err() {
            return;
        }
    }
}
