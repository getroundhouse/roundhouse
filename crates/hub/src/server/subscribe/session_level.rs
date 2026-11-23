//! A subscription at a session's level: where the session's own records stand, then its events from where the client
//! says, its tail, and every event, delta and notice of the session's records after.

use std::sync::Arc;

use protocol::client::ApiError;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::LogFrom;
use protocol::model::{Place, Seq, SessionId};
use tokio::sync::mpsc;

use super::notices::{forward, send_listed};
use super::starting::{Feeding, unknown};
use crate::hub::Hub;
use crate::server::pump::{Following, pump};
use crate::server::sends::Sends;
use crate::server::{Closer, Outgoing};

/// What feeds a subscription at session `id`'s level, from `from`, once the hub has the session and its records, so
/// one it doesn't have is refused.
pub(super) async fn feeding(
    hub: &Arc<Hub>,
    id: SessionId,
    from: Option<LogFrom>,
    sends: Sends,
    subscription_id: u64,
    frames: mpsc::Sender<Outgoing>,
    closer: Closer,
) -> Result<Feeding, ApiError> {
    let following = match from {
        None => Following::Log { next: Seq::FIRST },
        Some(LogFrom::Seq { seq }) => Following::Log { next: seq.max(Seq::FIRST) },
        Some(LogFrom::Place { subagent, place }) => {
            Following::places(hub, &id, subagent, place.max(Place::FIRST)).await?
        }
        Some(LogFrom::Unknown) => return Err(unknown("a place in a log")),
    };
    let session = hub.live_session(&id).await?;
    // Before the list, so nothing that changes after it is missed.
    let feed = hub.subscribe_feed();
    let records = hub.session_records(id.clone()).await?;
    let live = session.subscribe();
    let hub = hub.clone();
    Ok(Box::pin(async move {
        // First, so a chat has them before the events that draw them.
        if !send_listed(&frames, &sends, subscription_id, Listed::Session(records)).await {
            return;
        }
        let list = || async { Ok(Listed::Session(hub.session_records(id.clone()).await?)) };
        let records = forward(feed, &sends, subscription_id, &frames, &closer, list, |_| {});
        let log = pump(hub.clone(), session, id.clone(), subscription_id, following, live, frames.clone());
        // The log's ends once its session has gone, and both once the connection has.
        tokio::select! {
            () = records => {}
            () = log => {}
        }
    }))
}
