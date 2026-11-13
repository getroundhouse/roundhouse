use crate::hub::Hub;
use crate::session::{Live, LiveItem, LiveSession};
use protocol::client::HubFrame;
use protocol::event::SessionEvent;
use protocol::model::{Place, Seq, SessionId, ToolUseId};
use protocol::tail::Tail;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::warn;

/// What a subscription follows, and how far it has been sent.
pub(in super::super) enum Following {
    /// Every event, and the next `seq` to send.
    Log { next: Seq },
    /// The agent's own events, or those of the subagent `subagent` started, and the next place to send. Its tail and
    /// deltas are only what that one is writing.
    Places { subagent: Option<ToolUseId>, next: Place },
}

impl Following {
    /// Whether what the agent, or the subagent `subagent` started, records or writes is part of this.
    pub(super) fn takes(&self, subagent: Option<&ToolUseId>) -> bool {
        match self {
            Following::Log { .. } => true,
            Following::Places { subagent: followed, .. } => followed.as_ref() == subagent,
        }
    }

    pub(super) fn tail(&self, tail: Tail) -> Tail {
        match self {
            Following::Log { .. } => tail,
            Following::Places { subagent, .. } => tail.of(subagent.as_ref()),
        }
    }

    pub(super) fn sent(&mut self, event: &SessionEvent) {
        match self {
            Following::Log { next } => *next = event.seq.next(),
            Following::Places { next, .. } => *next = event.place.next(),
        }
    }
}

/// Feeds one subscription. The log is always the source of durable events: live items only say that the log has
/// moved, so a slow client can fall behind and catch up without losing anything.
pub(in super::super) async fn pump(
    hub: Arc<Hub>,
    session: Arc<LiveSession>,
    id: SessionId,
    subscription_id: u64,
    mut following: Following,
    mut live: broadcast::Receiver<Live>,
    frames: mpsc::Sender<HubFrame>,
) {
    'resync: loop {
        let snapshot = session.snapshot();
        if send_through(&hub, &id, subscription_id, &mut following, snapshot.last_seq, &frames).await.is_err() {
            return;
        }
        let tail = following.tail(snapshot.tail);
        if frames.send(HubFrame::Tail { subscription_id, tail }).await.is_err() {
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
                LiveItem::Appended { seq, subagent, .. } if following.takes(subagent.as_ref()) => {
                    send_through(&hub, &id, subscription_id, &mut following, seq, &frames).await
                }
                // The client never sees the event, so a new snapshot gives it the tail that's left.
                LiveItem::Appended { ended_others: true, .. } | LiveItem::Rewritten => continue 'resync,
                LiveItem::Delta(delta) if following.takes(delta.parent_tool_use_id.as_ref()) => {
                    frames.send(HubFrame::Delta { subscription_id, delta }).await.map_err(|_| ())
                }
                // What another records or writes, which this doesn't follow.
                LiveItem::Appended { .. } | LiveItem::Delta(_) => Ok(()),
            };
            if sent.is_err() {
                return;
            }
        }
    }
}

/// Sends what's followed of the log as far as `through`.
async fn send_through(
    hub: &Arc<Hub>,
    id: &SessionId,
    subscription_id: u64,
    following: &mut Following,
    through: Seq,
    frames: &mpsc::Sender<HubFrame>,
) -> Result<(), ()> {
    loop {
        // Stopping at `through` matters. An event read early would overtake the live deltas that came
        // before it, and those would then land on a tail the event had already cleared.
        let page = match following {
            Following::Log { next } if *next > through => return Ok(()),
            Following::Log { next } => hub.read_page(id.clone(), *next, through).await,
            Following::Places { subagent, next } => {
                hub.read_places_page(id.clone(), subagent.clone(), *next, through).await
            }
        };
        let page = page.map_err(|e| warn!(session = %id, "reading the log for a subscriber failed: {e}"))?;
        if page.is_empty() {
            return Ok(());
        }
        for event in page {
            following.sent(&event);
            frames.send(HubFrame::Event { subscription_id, event }).await.map_err(|_| ())?;
        }
    }
}
