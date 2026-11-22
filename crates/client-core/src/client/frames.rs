//! Reading what the hub sends on a connection, on the connection's own thread: each reply goes to whoever is waiting
//! for it, each subscription's items to its channel, and once the connection ends, everyone waiting hears so.

use std::io::{BufRead, BufReader};
use std::sync::Arc;

use protocol::client::{HubFrame, ReplyOutcome};
use protocol::frame;

use super::connection::{ClientError, Inner, SubscriptionItem};

/// Reads `stream`, the connection's, until it ends, then ends the connection.
pub(super) fn read_frames(inner: &Arc<Inner>, stream: impl std::io::Read) {
    let mut reader = BufReader::new(stream);
    let mut line = Vec::new();
    loop {
        frame::clear_buffer(&mut line);
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let parsed = match frame::decode::<HubFrame>(&line) {
            Ok(parsed) => parsed,
            Err(e) => {
                fail_unreadable_reply(inner, &line, &e);
                continue;
            }
        };
        match parsed {
            HubFrame::Reply { id, outcome } => {
                if let Some(reply) = inner.pending.lock().remove(&id) {
                    let _ = reply.send(match outcome {
                        ReplyOutcome::Response(response) => Ok(response),
                        ReplyOutcome::Error(error) => Err(ClientError::Api(error)),
                    });
                }
            }
            HubFrame::Event { subscription_id, event } => {
                deliver(inner, subscription_id, SubscriptionItem::Event(Box::new(event)));
            }
            HubFrame::Tail { subscription_id, tail } => deliver(inner, subscription_id, SubscriptionItem::Tail(tail)),
            HubFrame::Delta { subscription_id, delta } => {
                deliver(inner, subscription_id, SubscriptionItem::Delta(delta));
            }
            HubFrame::Notice { subscription_id, notice } => {
                deliver(inner, subscription_id, SubscriptionItem::Notice(Box::new(notice)));
            }
            HubFrame::Terminal { subscription_id, update } => {
                deliver(inner, subscription_id, SubscriptionItem::Terminal(update));
            }
            HubFrame::Github { subscription_id, update } => {
                deliver(inner, subscription_id, SubscriptionItem::Github(Box::new(update)));
            }
            HubFrame::TerminalPicture { subscription_id, picture } => {
                deliver(inner, subscription_id, SubscriptionItem::TerminalPicture(Box::new(picture)));
            }
        }
    }
    end_connection(inner);
}

pub(super) fn end_connection(inner: &Inner) {
    *inner.disconnected.lock() = true;
    let pending: Vec<_> = inner.pending.lock().drain().collect();
    for (_, reply) in pending {
        let _ = reply.send(Err(ClientError::Disconnected));
    }
    // Dropping the senders closes each channel once what's in it has been read, and `ended` says why.
    inner.subscriptions.lock().clear();
}

/// A hub can be a different version, so a reply might not decode, and without this whoever made the
/// call would wait forever. Anything else this can't read gets skipped.
fn fail_unreadable_reply(inner: &Inner, line: &[u8], error: &serde_json::Error) {
    let Some(id) = frame::reply_id(line) else { return };
    if let Some(reply) = inner.pending.lock().remove(&id) {
        let _ = reply.send(Err(ClientError::Unexpected(format!("a reply this client can't read: {error}"))));
    }
}

fn deliver(inner: &Inner, subscription_id: u64, item: SubscriptionItem) {
    let mut subscriptions = inner.subscriptions.lock();
    let Some(subscription) = subscriptions.get(&subscription_id) else { return };
    if let Err(flume::TrySendError::Full(_)) = subscription.try_send(item) {
        // Its reader has fallen behind. Marked first, so its reader never sees the channel close
        // without knowing why. Ending it closes the channel once what's in it has been read.
        inner.lagged.lock().insert(subscription_id);
        subscriptions.remove(&subscription_id);
        drop(subscriptions);
        let _ = inner.unsubscribe(subscription_id);
    }
}
