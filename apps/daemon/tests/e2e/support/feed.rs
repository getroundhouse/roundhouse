//! Waiting on what the feed says.

use std::time::Instant;

use client_core::{Subscription, SubscriptionItem};
use protocol::client::HubNotice;

use super::TIMEOUT;

/// The next notice the feed sends.
pub(crate) fn next_notice(feed: &Subscription, what: &str) -> HubNotice {
    next_notice_of(feed, what, |_| true)
}

/// The next notice the feed sends that `waited_for` takes, passing over the rest, for a test that waits for notices
/// of what it does, among those the hub sends whenever their own work gets there, such as of the GitHub login found on
/// the local provider's machine, which comes once the hub's first worker there has said.
pub(crate) fn next_notice_of(feed: &Subscription, what: &str, waited_for: impl Fn(&HubNotice) -> bool) -> HubNotice {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        match feed.items.recv_timeout(wait) {
            Ok(SubscriptionItem::Notice(notice)) if waited_for(&notice) => return *notice,
            Ok(_) => {}
            Err(e) => panic!("timed out waiting for {what}: {e}"),
        }
    }
}
