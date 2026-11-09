//! The connection to a hub.

mod admin;
mod api;
mod devices;
mod feed;
mod follow;
mod open_session;
mod remote;
mod start;
mod transport;

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use protocol::PROTOCOL_VERSION;
use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubNotice, ReplyOutcome,
};
use protocol::event::SessionEvent;
use protocol::frame;
use protocol::pairing::LinkError;
use protocol::tail::{StreamDelta, Tail};

pub use admin::{HubAdmin, stop_hub};
pub use api::{new_message_id, new_request_id};
pub use feed::RecordsFeed;
pub use follow::FollowError;
pub use open_session::OpenSession;
#[cfg(feature = "testing")]
pub use remote::WriteOnly;
pub use remote::{RemoteHub, pair};
pub use start::Daemon;
use transport::{Closer, Transport};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("the connection to the hub was lost")]
    Disconnected,
    #[error("unexpected reply from the hub: {0}")]
    Unexpected(String),
    #[error(transparent)]
    Link(#[from] LinkError),
    /// A remote hub didn't answer with its key at any of its hosts, and some didn't answer at all, as when its
    /// machine is asleep or this device is on another network.
    #[error("couldn't reach the hub at {}{}", hosts.join(", "), answered_with_another_key(not_pinned))]
    Unreachable {
        hosts: Vec<String>,
        #[source]
        last: Option<std::io::Error>,
        /// Hosts that did answer, but with a key other than the hub's, such as another machine that now has an
        /// address the hub had.
        not_pinned: Vec<String>,
    },
    /// Every host of a remote hub answered with a key other than the one this device pinned when it paired. Either
    /// the hub made a new certificate, and the device needs pairing again, or it isn't the hub.
    #[error(
        "the hub at {host} isn't the one this device paired with, because its key doesn't match. If the hub made a new certificate, pair again"
    )]
    FingerprintMismatch { host: String },
}

fn answered_with_another_key(not_pinned: &[String]) -> String {
    if not_pinned.is_empty() {
        return String::new();
    }
    format!(". {} answered, but with another key", not_pinned.join(", "))
}

impl ClientError {
    /// The hub is too old to know this request. Hide or disable whatever needed it.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, ClientError::Api(e) if e.code == ErrorCode::Unsupported)
    }

    /// There's a real break between this client and the hub, so only [`HubAdmin`] can reach it.
    pub fn is_version_mismatch(&self) -> bool {
        matches!(self, ClientError::Api(e) if e.code == ErrorCode::VersionMismatch)
    }

    /// A remote hub refused this device's pairing code or token. A device that was paired has been revoked, and
    /// needs pairing again.
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, ClientError::Api(e) if e.code == ErrorCode::Unauthorized)
    }
}

pub type Result<T> = std::result::Result<T, ClientError>;

/// What a subscription delivers. When it ends, its channel closes, and [`Subscription::ended`] says why.
/// To carry on, subscribe again from one past the last event's `seq`.
#[derive(Debug, Clone, PartialEq)]
pub enum SubscriptionItem {
    Event(SessionEvent),
    /// Replaces any streaming tail held so far.
    Tail(Tail),
    Delta(StreamDelta),
    /// Only on a feed subscription.
    Notice(HubNotice),
}

/// Why a subscription's channel closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    /// Its reader fell behind. Subscribing again on the same connection carries on.
    FellBehind,
    /// The connection to the hub has gone.
    Disconnected,
}

/// Far more than a reader that keeps up ever has waiting. A subscription that fills it has fallen behind,
/// and ends, so its reader can start again from where it got to rather than queue without limit.
const SUBSCRIPTION_CAPACITY: usize = 4096;

/// Dropping it tells the hub to stop sending.
pub struct Subscription {
    pub id: u64,
    pub items: flume::Receiver<SubscriptionItem>,
    connection: Arc<Inner>,
}

impl Subscription {
    /// Why it ended, once its channel has closed. `None` while it's still going.
    pub fn ended(&self) -> Option<Ended> {
        if self.connection.lagged.lock().contains(&self.id) {
            Some(Ended::FellBehind)
        } else if *self.connection.disconnected.lock() {
            Some(Ended::Disconnected)
        } else {
            None
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        // In this order, so a delivery under way can't mark it as fallen behind after it's gone.
        self.connection.subscriptions.lock().remove(&self.id);
        self.connection.lagged.lock().remove(&self.id);
        // Not waiting for the reply: this can run on any thread, and the reply doesn't matter.
        let _ = self.connection.send_only(ClientRequest::SessionUnsubscribe { subscription_id: self.id });
    }
}

type PendingReply = flume::Sender<std::result::Result<ClientResponse, ClientError>>;

struct Inner {
    writer: Mutex<Box<dyn Write + Send>>,
    pending: Mutex<HashMap<u64, PendingReply>>,
    subscriptions: Mutex<HashMap<u64, flume::Sender<SubscriptionItem>>>,
    /// Subscriptions ended because their readers fell behind.
    lagged: Mutex<HashSet<u64>>,
    next_id: AtomicU64,
    /// Set once the reader has finished, so a later call can't wait forever.
    disconnected: Mutex<bool>,
    /// Set by the hello, before anyone else has this connection.
    hub_version: OnceLock<String>,
}

impl Inner {
    fn send_only(&self, request: ClientRequest) -> Result<()> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let line = frame::encode(&ClientFrame { id, request }).map_err(|e| ClientError::Unexpected(e.to_string()))?;
        self.writer.lock().write_all(&line)?;
        Ok(())
    }
}

/// Dropping the last clone disconnects, which also ends its subscriptions. A connection to a hub on this machine
/// and one to a hub over the network behave the same.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
    /// Closes the connection when the last clone goes away. The reader thread can't be what keeps this alive, or
    /// it would wait forever on a socket that only it was holding open.
    _closer: Arc<Closer>,
}

impl Client {
    /// Connects to the hub on this machine, and checks the protocol version.
    pub fn connect(socket: &Path, client_name: &str) -> Result<Client> {
        let client = Client::open(socket)?;
        client.hello(client_name)?;
        Ok(client)
    }

    /// A connection that hasn't said hello, which a hub only answers a little on. See [`HubAdmin`].
    fn open(socket: &Path) -> Result<Client> {
        Client::start(Transport::unix(socket)?)
    }

    fn hello(&self, client_name: &str) -> Result<()> {
        let reply =
            self.call(ClientRequest::Hello { protocol_version: PROTOCOL_VERSION, client: client_name.to_string() })?;
        let ClientResponse::Hello { hub_version, .. } = reply else {
            return unexpected(&reply);
        };
        let _ = self.inner.hub_version.set(hub_version);
        Ok(())
    }

    /// Starts reading what the hub sends on its own thread.
    fn start(transport: Transport) -> Result<Client> {
        let Transport { reader, writer, closer } = transport;
        let inner = Arc::new(Inner {
            writer: Mutex::new(writer),
            pending: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
            lagged: Mutex::new(HashSet::new()),
            next_id: AtomicU64::new(1),
            disconnected: Mutex::new(false),
            hub_version: OnceLock::new(),
        });
        let for_reader = inner.clone();
        std::thread::Builder::new()
            .name("hub-client-reader".to_string())
            .spawn(move || read_frames(&for_reader, reader))?;
        Ok(Client { inner, _closer: Arc::new(closer) })
    }

    /// Which build the hub is, as it said when this connected.
    pub fn hub_version(&self) -> &str {
        self.inner.hub_version.get().map_or("", String::as_str)
    }

    pub fn call(&self, request: ClientRequest) -> Result<ClientResponse> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let line = frame::encode(&ClientFrame { id, request }).map_err(|e| ClientError::Unexpected(e.to_string()))?;
        let (reply_tx, reply_rx) = flume::bounded(1);
        self.inner.pending.lock().insert(id, reply_tx);
        if *self.inner.disconnected.lock() {
            self.inner.pending.lock().remove(&id);
            return Err(ClientError::Disconnected);
        }
        // Writing to the hub only fails once it has gone.
        if self.inner.writer.lock().write_all(&line).is_err() {
            self.inner.pending.lock().remove(&id);
            return Err(ClientError::Disconnected);
        }
        reply_rx.recv().map_err(|_| ClientError::Disconnected)?
    }
}

fn unexpected<T>(response: &ClientResponse) -> Result<T> {
    Err(ClientError::Unexpected(format!("{response:?}")))
}

fn read_frames(inner: &Arc<Inner>, stream: impl Read) {
    let mut reader = BufReader::new(stream);
    let mut line = Vec::new();
    loop {
        line.clear();
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
                deliver(inner, subscription_id, SubscriptionItem::Event(event));
            }
            HubFrame::Tail { subscription_id, tail } => deliver(inner, subscription_id, SubscriptionItem::Tail(tail)),
            HubFrame::Delta { subscription_id, delta } => {
                deliver(inner, subscription_id, SubscriptionItem::Delta(delta));
            }
            HubFrame::Notice { subscription_id, notice } => {
                deliver(inner, subscription_id, SubscriptionItem::Notice(notice));
            }
        }
    }
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
    #[derive(serde::Deserialize)]
    struct AnyReply {
        id: u64,
    }
    let Ok(AnyReply { id }) = frame::decode(line) else {
        return;
    };
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
        let _ = inner.send_only(ClientRequest::SessionUnsubscribe { subscription_id });
    }
}

/// A channel for one subscription's items, limited to what a reader that keeps up could have waiting.
fn subscription_channel() -> (flume::Sender<SubscriptionItem>, flume::Receiver<SubscriptionItem>) {
    flume::bounded(SUBSCRIPTION_CAPACITY)
}

/// Nothing's there, or what was there went away as we connected: a hub that's exiting can accept a
/// connection and then drop it, and so can one that loses the race with another starting at once.
fn hub_is_missing(error: &ClientError) -> bool {
    match error {
        ClientError::Disconnected | ClientError::Unreachable { .. } => true,
        ClientError::Io(e) => matches!(
            e.kind(),
            std::io::ErrorKind::NotFound
                | std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::BrokenPipe
        ),
        ClientError::Api(_)
        | ClientError::Unexpected(_)
        | ClientError::Link(_)
        | ClientError::FingerprintMismatch { .. } => false,
    }
}

#[cfg(test)]
mod tests;
