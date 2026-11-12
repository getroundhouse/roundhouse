//! Waiting on what the feed says.

use std::time::Instant;

use client_core::{Subscription, SubscriptionItem};
use protocol::client::HubNotice;

use super::TIMEOUT;

/// The next notice the feed sends.
pub(crate) fn next_notice(feed: &Subscription, what: &str) -> HubNotice {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        match feed.items.recv_timeout(wait) {
            Ok(SubscriptionItem::Notice(notice)) => return *notice,
            Ok(_) => {}
            Err(e) => panic!("timed out waiting for {what}: {e}"),
        }
    }
}
