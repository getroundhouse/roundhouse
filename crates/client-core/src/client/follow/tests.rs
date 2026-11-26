//! The follow loop, on a source made up for these tests, letting its subscription go while it isn't wanted.
#![cfg(unix)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{ClientRequest, ClientResponse, ReplyOutcome};

use super::{Following, Source};
use crate::client::{Client, Result, Subscription, SubscriptionItem};
use crate::stand_in_hub::{Does, listen, subscribed};

const WAIT: Duration = Duration::from_secs(10);

/// What the source has been through, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Seen {
    Resumed,
    Reconnected,
    /// Asked to do what was asked of it, with a connection or without.
    Asked {
        connected: bool,
    },
}

/// Until when it's wanted, which the test sets.
#[derive(Default)]
struct Wanted(Option<Instant>);

/// Follows a terminal, as the simplest subscription there is, and says what it's been through.
struct Made {
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Source for Made {
    type Held = Wanted;

    fn resume(&mut self, client: &Client, _: &Mutex<Wanted>) -> Result<Subscription> {
        self.seen.lock().push(Seen::Resumed);
        client.follow(Follow::Terminal { id: "t".into() })
    }

    fn apply(&mut self, _: &Client, _: &Mutex<Wanted>, _: SubscriptionItem) -> Result<bool> {
        Ok(false)
    }

    fn asked(&mut self, connection: Option<&Client>, _: &Mutex<Wanted>, _: &dyn Fn()) -> Result<bool> {
        self.seen.lock().push(Seen::Asked { connected: connection.is_some() });
        Ok(false)
    }

    fn reconnected(&mut self) {
        self.seen.lock().push(Seen::Reconnected);
    }

    fn wanted_until(&self, held: &Mutex<Wanted>) -> Option<Instant> {
        held.lock().0
    }
}

/// A hub that answers each request, and says what each was.
fn start_hub(socket: &std::path::Path) -> flume::Receiver<ClientRequest> {
    let (asked, told) = flume::unbounded();
    listen(socket, move |request| {
        let does = if let ClientRequest::Subscribe(subscribe) = &request {
            subscribed(subscribe, |_, _| Vec::new())
        } else {
            Does::Answers(Box::new(ReplyOutcome::Response(ClientResponse::Ok)))
        };
        let _ = asked.send(request);
        does
    });
    told
}

/// Whether `request` starts following the terminal, and stops nothing.
fn starts(request: &ClientRequest) -> bool {
    let ClientRequest::Subscribe(Subscribe { stop, start }) = request else { return false };
    stop.is_empty() && matches!(start.as_slice(), [one] if one.follow == Follow::Terminal { id: "t".into() })
}

fn next(asked: &flume::Receiver<ClientRequest>, what: &str) -> ClientRequest {
    asked.recv_timeout(WAIT).unwrap_or_else(|_| panic!("{what} never came"))
}

fn want_until(follow: &Following<Wanted>, until: Option<Instant>) {
    follow.quietly(|wanted| wanted.0 = until);
    follow.wake();
}

/// Once it isn't wanted, its subscription goes, as the hub hears. A new connection meanwhile follows nothing there,
/// while what's asked of it is still done on that connection, and it follows there once it's wanted again.
#[test]
fn a_source_not_wanted_is_let_go_until_it_is_wanted_again_on_the_connection_it_then_has() {
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("first.sock"), dir.path().join("second.sock"));
    let (first_asked, second_asked) = (start_hub(&first), start_hub(&second));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let made = Made { seen: seen.clone() };
    let follow = Following::start(Some(&Client::connect(&first, "test").unwrap()), Wanted::default(), move || made);
    assert!(starts(&next(&first_asked, "the first watch")));

    want_until(&follow, Some(Instant::now() + Duration::from_millis(50)));
    let let_go = next(&first_asked, "letting go once it wasn't wanted");
    follow.reconnect(&Client::connect(&second, "test").unwrap());
    waiting::thread::until(WAIT, "the new connection was never asked on", || {
        seen.lock().ends_with(&[Seen::Reconnected, Seen::Asked { connected: true }])
    });
    let after_reconnecting = seen.lock().clone();
    want_until(&follow, None);
    let watched_again = next(&second_asked, "the watch once it was wanted again");

    assert!(
        matches!(&let_go, ClientRequest::Subscribe(Subscribe { stop, start }) if stop.len() == 1 && start.is_empty())
    );
    assert_eq!(after_reconnecting.iter().filter(|seen| **seen == Seen::Resumed).count(), 1, "{after_reconnecting:?}");
    assert!(starts(&watched_again), "{watched_again:?}");
}
