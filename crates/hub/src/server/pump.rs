//! Feeding subscriptions: a session's log and live output, and the hub-wide feed.

use std::sync::Arc;

use protocol::client::{HubFrame, HubNotice};
use protocol::model::{Seq, SessionId};
use tokio::sync::{broadcast, mpsc};
use tracing::warn;

use crate::hub::Hub;
use crate::session::{Live, LiveItem, LiveSession};

pub(super) async fn pump_feed(
    mut feed: broadcast::Receiver<HubNotice>,
    subscription_id: u64,
    frames: mpsc::Sender<HubFrame>,
) {
    loop {
        let notice = match feed.recv().await {
            Ok(notice) => notice,
            Err(broadcast::error::RecvError::Lagged(_)) => HubNotice::Resync,
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if frames.send(HubFrame::Notice { subscription_id, notice }).await.is_err() {
            return;
        }
    }
}

/// Feeds one subscription. The log is always the source of durable events: live items only say
/// that the log has moved, so a slow client can fall behind and catch up without losing anything.
pub(super) async fn pump(
    hub: Arc<Hub>,
    session: Arc<LiveSession>,
    id: SessionId,
    subscription_id: u64,
    from_seq: Seq,
    mut live: broadcast::Receiver<Live>,
    frames: mpsc::Sender<HubFrame>,
) {
    let mut cursor = from_seq.max(1);
    'resync: loop {
        let snapshot = session.snapshot();
        if send_log_through(&hub, &id, subscription_id, &mut cursor, snapshot.last_seq, &frames).await.is_err() {
            return;
        }
        if frames.send(HubFrame::Tail { subscription_id, tail: snapshot.tail }).await.is_err() {
            return;
        }
        loop {
            let item = match live.recv().await {
                Ok(item) => item,
                // Missed live items. The log has every durable event, and a new snapshot replaces
                // the tail, so this loses nothing.
                Err(broadcast::error::RecvError::Lagged(_)) => continue 'resync,
                Err(broadcast::error::RecvError::Closed) => return,
            };
            if item.version <= snapshot.version {
                continue;
            }
            let sent = match item.item {
                LiveItem::Appended(seq) => {
                    send_log_through(&hub, &id, subscription_id, &mut cursor, seq, &frames).await
                }
                LiveItem::Delta(delta) => frames.send(HubFrame::Delta { subscription_id, delta }).await.map_err(|_| ()),
            };
            if sent.is_err() {
                return;
            }
        }
    }
}

async fn send_log_through(
    hub: &Arc<Hub>,
    id: &SessionId,
    subscription_id: u64,
    cursor: &mut Seq,
    through: Seq,
    frames: &mpsc::Sender<HubFrame>,
) -> Result<(), ()> {
    while *cursor <= through {
        // Stopping at `through` matters. An event read early would overtake the live deltas that came
        // before it, and those would then land on a tail the event had already cleared.
        let page = hub.read_page(id.clone(), *cursor, through).await.map_err(|e| {
            warn!(session = %id, "reading the log for a subscriber failed: {e}");
        })?;
        if page.is_empty() {
            return Ok(());
        }
        for event in page {
            *cursor = event.seq + 1;
            frames.send(HubFrame::Event { subscription_id, event }).await.map_err(|_| ())?;
        }
    }
    Ok(())
}
