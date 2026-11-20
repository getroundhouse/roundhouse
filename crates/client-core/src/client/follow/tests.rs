// Against a stand-in hub on a Unix socket.

use std::time::{Duration, Instant};

use parking_lot::Mutex;
use protocol::client::{ClientResponse, ReplyOutcome};

use super::{Follow, Source};
use crate::client::{Client, ClientError, Result, Subscription, SubscriptionItem};
use crate::stand_in_hub::{Does, connect};

const WAIT: Duration = Duration::from_secs(10);

/// A source with something due a moment after it starts, once, which says each time it's asked when there's next
/// something due, and when that's done, and when it's asked to do what it was asked.
struct Chore {
    subscribes: bool,
    due_at: Option<Instant>,
    asked_when: flume::Sender<&'static str>,
}

impl Source for Chore {
    type Held = ();

    fn resume(&mut self, client: &Client, _: &Mutex<()>) -> Result<Subscription> {
        if self.subscribes { client.terminal_attach(&"t".into()) } else { Err(ClientError::Disconnected) }
    }

    fn apply(&mut self, _: &Client, _: &Mutex<()>, _: SubscriptionItem) -> Result<bool> {
        Ok(false)
    }

    fn asked(&mut self, _: Option<&Client>, _: &Mutex<()>, _: &dyn Fn()) -> Result<bool> {
        let _ = self.asked_when.send("asked");
        Ok(false)
    }

    fn due_at(&mut self) -> Option<Instant> {
        let _ = self.asked_when.send("when");
        self.due_at
    }

    fn due(&mut self) {
        let _ = self.asked_when.send("due");
        self.due_at = None;
    }
}

/// Once it's due, whether or not there's a subscription, and then not again until it says so, rather than going round
/// and round asking.
#[test]
fn the_thread_does_what_its_source_says_is_due_once_it_is() {
    for subscribes in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let client = connect(&dir.path().join("hub.sock"), |_| {
            Does::Answers(Box::new(ReplyOutcome::Response(ClientResponse::Ok)))
        });
        let (asked_when, told) = flume::unbounded();
        let due_at = Some(Instant::now() + Duration::from_millis(20));
        let follow = Follow::start(&client, (), move || Chore { subscribes, due_at, asked_when });

        assert_eq!(told.recv_timeout(WAIT).unwrap(), "when");
        assert_eq!(told.recv_timeout(WAIT).unwrap(), "due", "subscribed: {subscribes}");
        follow.wake();
        let then: Vec<_> = [(); 3].map(|()| told.recv_timeout(WAIT).unwrap()).into();
        assert_eq!(then, ["when", "asked", "when"], "subscribed: {subscribes}");
    }
}
