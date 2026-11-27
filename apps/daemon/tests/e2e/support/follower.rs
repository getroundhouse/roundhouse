//! What a client holds of a session it follows.

use std::time::Instant;

use client_core::{Subscription, SubscriptionItem};
use protocol::event::SessionEvent;
use protocol::tail::Tail;

use super::TIMEOUT;

/// What a client would hold after reading a subscription: its events, and its streaming tail.
#[derive(Default)]
pub(crate) struct Follower {
    pub(crate) events: Vec<SessionEvent>,
    pub(crate) tail: Tail,
}

impl Follower {
    pub(crate) fn apply(&mut self, item: SubscriptionItem) {
        match item {
            SubscriptionItem::Event(event) => {
                self.tail.on_event(event.seq, event.run, &event.body);
                self.events.push(*event);
            }
            SubscriptionItem::Tail(tail) => self.tail = tail,
            SubscriptionItem::Delta(delta) => self.tail.apply(&delta),
            SubscriptionItem::Notice(_)
            | SubscriptionItem::Listed(_)
            | SubscriptionItem::Terminal(_)
            | SubscriptionItem::TerminalPicture(_) => {}
        }
    }

    pub(crate) fn drain_until(
        &mut self,
        subscription: &Subscription,
        what: &str,
        mut done: impl FnMut(&Follower) -> bool,
    ) {
        let deadline = Instant::now() + TIMEOUT;
        while !done(self) {
            let wait = deadline.saturating_duration_since(Instant::now());
            match subscription.items.recv_timeout(wait) {
                Ok(item) => self.apply(item),
                Err(e) => panic!("timed out waiting for {what}: {e}"),
            }
        }
    }

    pub(crate) fn tail_text(&self) -> String {
        self.tail.messages.iter().flat_map(|m| &m.blocks).map(|b| b.text.as_str()).collect()
    }
}
