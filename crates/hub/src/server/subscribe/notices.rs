//! What every level that holds records sends after where it stands: each notice of its level, and where it stands again
//! whenever it has fallen behind.

use std::future::Future;
use std::sync::Arc;

use protocol::client::listed::Listed;
use protocol::client::{ApiError, HubFrame, HubNotice};
use tokio::sync::{broadcast, mpsc};
use tracing::warn;

use crate::hub::Fed;
use crate::server::sends::Sends;
use crate::server::{Closer, Outgoing};

/// Sends subscription `subscription_id` where it stands, `listed`, before anything else it's sent, as `sends` has it
/// sent. False once the connection has gone.
pub(super) async fn send_listed(
    frames: &mpsc::Sender<Outgoing>,
    sends: &Sends,
    subscription_id: u64,
    listed: Listed,
) -> bool {
    frames.send(HubFrame::Listed { subscription_id, listed: sends.listed(listed) }.into()).await.is_ok()
}

/// Sends subscription `subscription_id` each notice of `feed` that `sends` has it sent, telling `seen` of each, until
/// the connection goes. Whenever it falls behind the feed, it's sent where it stands again, as `list` reads it, in place
/// of the notices it missed, as `sends` has it sent. Where that list can't be read, as when the database fails or a
/// session is deleted meanwhile, `closer` closes the connection, as the log says, so the client follows everything
/// again, rather than going on without what it missed.
pub(super) async fn forward<F: Future<Output = Result<Listed, ApiError>>>(
    mut feed: broadcast::Receiver<Fed>,
    sends: &Sends,
    subscription_id: u64,
    frames: &mpsc::Sender<Outgoing>,
    closer: &Closer,
    mut list: impl FnMut() -> F,
    mut seen: impl FnMut(&HubNotice),
) {
    loop {
        let frame = match feed.recv().await {
            Ok(told) if sends.notice(&told) => {
                seen(&told.notice);
                HubFrame::Notice { subscription_id, notice: Arc::unwrap_or_clone(told.notice) }
            }
            Ok(_) => continue,
            Err(broadcast::error::RecvError::Lagged(_)) => match list().await {
                Ok(listed) => HubFrame::Listed { subscription_id, listed: sends.listed(listed) },
                Err(error) => {
                    let message = error.message;
                    warn!(
                        ?sends,
                        "closing the connection of a subscriber that fell behind and can't be listed again: {message}"
                    );
                    closer.close();
                    return;
                }
            },
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if frames.send(frame.into()).await.is_err() {
            return;
        }
    }
}
