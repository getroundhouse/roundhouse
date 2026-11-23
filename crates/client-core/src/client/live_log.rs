//! A session's log from now on, item by item, in the order the hub sends each, for whatever has to see every item
//! rather than the log as it stands, such as reading replies out as they come.

use std::collections::VecDeque;

use parking_lot::Mutex;
use protocol::event::SessionEvent;
use protocol::model::{Seq, Session, SessionId};
use protocol::tail::{StreamDelta, Tail};

use super::follow::{Following, Source};
use super::{Client, Result, Subscription, SubscriptionItem};

/// One of what the hub sent, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum LiveItem {
    /// An event recorded before the subscription first caught up, which isn't live.
    Before(Box<SessionEvent>),
    /// The tail, as the hub sent it once it had caught up, the first time, and again when it rewrote it. What comes
    /// after the first is live.
    Tail(Box<Tail>),
    /// An event recorded once it had caught up.
    Event(Box<SessionEvent>),
    Delta(Box<StreamDelta>),
}

/// The items not taken yet, and whether the subscription has caught up.
#[derive(Default)]
struct Items {
    items: VecDeque<LiveItem>,
    caught_up: bool,
}

/// A session's log from when it was made, on a thread of its own, carrying on from where it got to on a new connection.
#[derive(Clone)]
pub struct LiveLog {
    follow: Following<Items>,
}

impl LiveLog {
    /// Starts following `session` from the event after its last as it's recorded here.
    pub fn start(client: &Client, session: &Session) -> Self {
        let source = LiveSource { id: session.id.clone(), next: session.last_seq.next() };
        let follow = Following::start(client, Items::default(), move || source);
        follow.wake();
        Self { follow }
    }

    /// Says that there are items to take. It holds one signal at a time.
    pub fn changed(&self) -> &flume::Receiver<()> {
        &self.follow.changed
    }

    /// The items that came since this was last asked, in order.
    pub fn take(&self) -> Vec<LiveItem> {
        self.follow.quietly(|held| held.items.drain(..).collect())
    }

    /// Carries on, on a new connection to the hub.
    pub fn reconnect(&self, client: &Client) {
        self.follow.reconnect(client);
    }
}

struct LiveSource {
    id: SessionId,
    /// The event to follow from: one past the last received.
    next: Seq,
}

impl Source for LiveSource {
    type Held = Items;

    fn resume(&mut self, client: &Client, _: &Mutex<Items>) -> Result<Subscription> {
        client.follow_session(&self.id, self.next)
    }

    fn apply(&mut self, _: &Client, held: &Mutex<Items>, item: SubscriptionItem) -> Result<bool> {
        let mut held = held.lock();
        let item = match item {
            SubscriptionItem::Event(event) => {
                self.next = self.next.max(event.seq.next());
                if held.caught_up { LiveItem::Event(event) } else { LiveItem::Before(event) }
            }
            SubscriptionItem::Tail(tail) => {
                held.caught_up = true;
                LiveItem::Tail(Box::new(tail))
            }
            SubscriptionItem::Delta(delta) => LiveItem::Delta(Box::new(delta)),
            // The session's own records, which an open session keeps, as `OpenSession` says.
            SubscriptionItem::Listed(_)
            | SubscriptionItem::Notice(_)
            | SubscriptionItem::Terminal(_)
            | SubscriptionItem::Github(_)
            | SubscriptionItem::TerminalPicture(_) => {
                return Ok(false);
            }
        };
        held.items.push_back(item);
        Ok(true)
    }

    fn asked(&mut self, _: Option<&Client>, _: &Mutex<Items>, _: &dyn Fn()) -> Result<bool> {
        Ok(false)
    }
}
