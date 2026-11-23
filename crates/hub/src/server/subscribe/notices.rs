//! What every level that holds records sends after where it stands: each notice of its level, and where it stands again
//! whenever it has fallen behind.

use std::future::Future;

use protocol::client::listed::Listed;
use protocol::client::{ApiError, HubFrame, HubNotice};
use protocol::model::Level;
use tokio::sync::{broadcast, mpsc};
use tracing::warn;

use crate::server::Outgoing;

/// Sends subscription `subscription_id` where it stands, `listed`, before anything else it's sent. False once the
/// connection has gone.
pub(super) async fn send_listed(frames: &mpsc::Sender<Outgoing>, subscription_id: u64, listed: Listed) -> bool {
    frames.send(HubFrame::Listed { subscription_id, listed }.into()).await.is_ok()
}

/// Sends subscription `subscription_id` each notice of `feed` that's of `level`, telling `seen` of each, until the
/// connection goes. Whenever it falls behind the feed, it's sent where it stands again, as `list` reads it, in place of
/// the notices it missed. A list that can't be read is said in the log, and the notices go on, so what changes after
/// reaches the client, rather than ending a subscription the client can't tell has ended.
pub(super) async fn forward<F: Future<Output = Result<Listed, ApiError>>>(
    mut feed: broadcast::Receiver<HubNotice>,
    level: &Level,
    subscription_id: u64,
    frames: &mpsc::Sender<Outgoing>,
    mut list: impl FnMut() -> F,
    mut seen: impl FnMut(&HubNotice),
) {
    loop {
        let frame = match feed.recv().await {
            Ok(notice) if notice.level().as_ref() == Some(level) => {
                seen(&notice);
                HubFrame::Notice { subscription_id, notice }
            }
            Ok(_) => continue,
            Err(broadcast::error::RecvError::Lagged(_)) => match list().await {
                Ok(listed) => HubFrame::Listed { subscription_id, listed },
                Err(error) => {
                    warn!(?level, "listing again for a subscriber that fell behind: {}", error.message);
                    continue;
                }
            },
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if frames.send(frame.into()).await.is_err() {
            return;
        }
    }
}
