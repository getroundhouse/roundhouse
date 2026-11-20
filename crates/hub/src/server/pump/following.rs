use crate::hub::Hub;
use crate::server::Outgoing;
use crate::session::{Live, LiveItem, LiveSession};
use protocol::client::{ApiError, HubFrame};
use protocol::event::SessionEvent;
use protocol::model::{Place, Seq, SessionId, SubagentId};
use protocol::tail::Tail;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

use tracing::warn;

/// What a subscription follows, and how far it has been sent.
pub(in super::super) enum Following {
    /// Every event, and the next `seq` to send.
    Log { next: Seq },
    /// The agent's own events, or those of the subagent `subagent`, and the next place to send. Its tail and
    /// deltas are only what that one is writing. `unrecorded` while the hub has no record of a subagent so named, as
    /// when a client names one by the call that started it before the agent has said it started.
    Places { subagent: Option<SubagentId>, next: Place, unrecorded: bool },
}

impl Following {
    /// The agent's own events, or those of the subagent `named` names, as the hub finds it, from `next` on.
    pub(in super::super) async fn places(
        hub: &Hub,
        id: &SessionId,
        named: Option<SubagentId>,
        next: Place,
    ) -> Result<Following, ApiError> {
        let Some(named) = named else { return Ok(Following::Places { subagent: None, next, unrecorded: false }) };
        let recorded = hub.recorded_subagent(id.clone(), named.clone()).await?;
        Ok(Following::Places { unrecorded: recorded.is_none(), subagent: Some(recorded.unwrap_or(named)), next })
    }

    /// Whether this now follows another subagent, the one its name names now that the hub has a record of it, as a
    /// client that named it by its call before the agent said it started means. Only while nothing of it has been
    /// sent, since then its name was its own.
    async fn recorded_now(&mut self, hub: &Hub, id: &SessionId) -> bool {
        let Following::Places { subagent: Some(named), next, unrecorded } = self else { return false };
        if !*unrecorded {
            return false;
        }
        if *next > Place::FIRST {
            *unrecorded = false;
            return false;
        }
        match hub.recorded_subagent(id.clone(), named.clone()).await {
            Ok(None) => false,
            Ok(Some(found)) => {
                *unrecorded = false;
                std::mem::replace(named, found) != *named
            }
            Err(e) => {
                warn!(session = %id, "finding the subagent a subscriber follows failed: {e}");
                false
            }
        }
    }

    /// Whether what the agent, or the subagent `subagent`, records or writes is part of this.
    pub(super) fn takes(&self, subagent: Option<&SubagentId>) -> bool {
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
    frames: mpsc::Sender<Outgoing>,
) {
    'resync: loop {
        // Its start may have been recorded since the subscription named it, before the snapshot, which the items for
        // it don't come after.
        following.recorded_now(&hub, &id).await;
        let snapshot = session.snapshot();
        if send_through(&hub, &id, subscription_id, &mut following, snapshot.last_seq, &frames).await.is_err() {
            return;
        }
        let tail = following.tail(snapshot.tail);
        if frames.send(HubFrame::Tail { subscription_id, tail }.into()).await.is_err() {
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
            // The agent's own events say when a subagent starts, which can give a record to the name this follows.
            if matches!(item.item, LiveItem::Appended { subagent: None, .. }) && following.recorded_now(&hub, &id).await
            {
                continue 'resync;
            }
            let sent = match item.item {
                LiveItem::Appended { seq, subagent, .. } if following.takes(subagent.as_ref()) => {
                    send_through(&hub, &id, subscription_id, &mut following, seq, &frames).await
                }
                // The client never sees the event, so a new snapshot gives it the tail that's left.
                LiveItem::Appended { ended_others: true, .. } | LiveItem::Rewritten => continue 'resync,
                LiveItem::Delta(delta) if following.takes(delta.subagent.as_ref()) => {
                    frames.send(HubFrame::Delta { subscription_id, delta }.into()).await.map_err(|_| ())
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
    frames: &mpsc::Sender<Outgoing>,
) -> Result<(), ()> {
    loop {
        // Stopping at `through` matters. An event read early would overtake the live deltas that came
        // before it, and those would then land on a tail the event had already cleared.
        let page = match following {
            Following::Log { next } if *next > through => return Ok(()),
            Following::Log { next } => hub.read_page(id.clone(), *next, through).await,
            Following::Places { subagent, next, .. } => {
                hub.read_places_page(id.clone(), subagent.clone(), *next, through).await
            }
        };
        let page = page.map_err(|e| warn!(session = %id, "reading the log for a subscriber failed: {e}"))?;
        if page.is_empty() {
            return Ok(());
        }
        for event in page {
            following.sent(&event);
            frames.send(HubFrame::Event { subscription_id, event }.into()).await.map_err(|_| ())?;
        }
    }
}
