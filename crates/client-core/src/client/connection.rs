use super::feed::Folding;
use super::transport::{Closer, Transport};
use super::{commands, lines, queueing};
use parking_lot::Mutex;
use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubNotice, Reply as _, ReplyOutcome,
    Request, subscriptions,
};
use protocol::event::SessionEvent;
use protocol::frame;
use protocol::model::{AttachmentId, SessionId};
use protocol::tail::{StreamDelta, Tail};
use protocol::terminal::TerminalUpdate;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("the connection to the hub was lost")]
    Disconnected,
    #[error("the hub at {host} has another key; pair again only after checking the hub")]
    FingerprintMismatch { host: String },
    #[error("the hub could not be reached at {hosts:?}: {last:?}")]
    Unreachable { hosts: Vec<String>, last: Option<std::io::Error>, not_pinned: Vec<String> },
    #[error("unexpected reply from the hub: {0}")]
    Unexpected(String),
}

impl ClientError {
    /// The hub is too old to know this request, or to read it. Hide or disable whatever needed it.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, ClientError::Api(e) if e.code == ErrorCode::Unsupported)
    }

    /// The hub has no such thing, such as a file it was never given or has let go of.
    pub fn is_not_found(&self) -> bool {
        matches!(self, ClientError::Api(e) if e.code == ErrorCode::NotFound)
    }

    /// There's a real break between this client and the hub, so only [`HubAdmin`](super::HubAdmin) can reach it.
    pub fn is_version_mismatch(&self) -> bool {
        matches!(self, ClientError::Api(e) if e.code == ErrorCode::VersionMismatch)
    }

    /// The hub refused our pairing code or device token.
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, ClientError::Api(e) if e.code == ErrorCode::Unauthorized)
    }
}

pub type Result<T> = std::result::Result<T, ClientError>;

/// What a subscription delivers. When it ends, its channel closes, and [`Subscription::ended`] says why.
/// To carry on, subscribe again from one past the last event's `seq`. An event and a notice are each boxed, since
/// every item in a subscription's channel takes the room of the largest, and one of them is the largest by far, which
/// one depending on the platform.
#[derive(Debug, Clone, PartialEq)]
pub enum SubscriptionItem {
    Event(Box<SessionEvent>),
    /// Replaces any streaming tail held so far.
    Tail(Tail),
    Delta(StreamDelta),
    /// Only on a feed subscription.
    Notice(Box<HubNotice>),
    /// Only on a terminal's.
    Terminal(TerminalUpdate),
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
pub(super) const SUBSCRIPTION_CAPACITY: usize = 4096;

/// Dropping it tells the hub to stop sending.
pub struct Subscription {
    pub id: u64,
    pub items: flume::Receiver<SubscriptionItem>,
    pub(super) connection: Arc<Inner>,
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
        let _ = self.connection.unsubscribe(self.id);
    }
}

type PendingReply = flume::Sender<std::result::Result<ClientResponse, ClientError>>;

pub(super) struct Inner {
    pub(super) writer: Mutex<Box<dyn Write + Send>>,
    pub(super) pending: Mutex<HashMap<u64, PendingReply>>,
    pub(super) subscriptions: Mutex<HashMap<u64, flume::Sender<SubscriptionItem>>>,
    /// Subscriptions ended because their readers fell behind.
    pub(super) lagged: Mutex<HashSet<u64>>,
    pub(super) next_id: AtomicU64,
    /// Set once the reader has finished, or a write has failed, so a later call can't wait forever.
    pub(super) disconnected: Mutex<bool>,
    /// Set by the hello, before anyone else has this connection.
    pub(super) hub_version: OnceLock<String>,
    pub(super) running: Arc<commands::Running>,
    /// Each chat's prompts, in the order they were written.
    pub(super) lines: Arc<lines::Lines>,
    /// The files this connection has added to a session, or read back from one, so a prompt that carries one doesn't
    /// send its bytes again.
    pub(super) given: Mutex<HashSet<(SessionId, AttachmentId)>>,
    /// Each chat's prompts being queued from here that the hub hasn't answered for yet.
    pub(super) queueing: Arc<queueing::Queueing>,
    pub(super) folding: Folding,
}

impl Inner {
    /// Asks the hub to stop following `subscription_id`, without waiting for its reply, which is dropped when it comes.
    pub(super) fn unsubscribe(&self, subscription_id: u64) -> Result<()> {
        self.send_only(subscriptions::SessionUnsubscribe { subscription_id }.into_request())
    }

    /// Sends `request` without waiting for its reply, which the reader drops, since nothing waits for it.
    pub(super) fn send_only(&self, request: ClientRequest) -> Result<()> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.send(id, request)
    }

    /// Sends `request` as call `id`, or says there's no connection to send it on.
    fn send(&self, id: u64, request: ClientRequest) -> Result<()> {
        let line = frame::encode(&ClientFrame { id, request }).map_err(|e| ClientError::Unexpected(e.to_string()))?;
        if *self.disconnected.lock() {
            return Err(ClientError::Disconnected);
        }
        // Writing to the hub only fails once it has gone, which the reader may not have found yet, so the connection
        // ends here.
        if self.writer.lock().write_all(&line).is_err() {
            end_connection(self);
            return Err(ClientError::Disconnected);
        }
        Ok(())
    }
}

/// Dropping the last clone disconnects, which also ends its subscriptions.
#[derive(Clone)]
pub struct Client {
    pub(super) inner: Arc<Inner>,
    pub(super) closer: Arc<Closer>,
}

impl Client {
    /// A connection that hasn't said hello, which a hub only answers a little on. See [`HubAdmin`](super::HubAdmin).
    pub(super) fn open(socket: &Path) -> Result<Client> {
        Self::from_transport(Transport::local(socket)?)
    }

    pub(super) fn from_transport(transport: Transport) -> Result<Client> {
        let Transport { reader, writer, closer } = transport;
        let closer = Arc::new(closer);
        let inner = Arc::new(Inner {
            writer: Mutex::new(writer),
            pending: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
            lagged: Mutex::new(HashSet::new()),
            next_id: AtomicU64::new(1),
            disconnected: Mutex::new(false),
            hub_version: OnceLock::new(),
            running: Arc::default(),
            lines: Arc::default(),
            given: Mutex::default(),
            queueing: Arc::default(),
            folding: Folding::default(),
        });
        let for_reader = inner.clone();
        std::thread::Builder::new()
            .name("hub-client-reader".to_string())
            .spawn(move || read_frames(&for_reader, reader))?;
        Ok(Client { inner, closer })
    }

    /// Which build the hub is, as it said when this connected.
    pub fn hub_version(&self) -> &str {
        self.inner.hub_version.get().map_or("", String::as_str)
    }

    /// Whether the hub has closed this connection, or gone. It's set before any request fails with
    /// [`ClientError::Disconnected`], and every request on it fails that way from then on.
    pub fn is_disconnected(&self) -> bool {
        *self.inner.disconnected.lock()
    }

    /// Sends `request` and waits for its reply, read as what the request names. A reply of any other kind fails it, with
    /// [`ClientError::Unexpected`]. Each feed following this connection has taken in any record it carries by then.
    pub fn call<R: Request>(&self, request: R) -> Result<R::Reply> {
        self.call_until(request, None)
    }

    pub(super) fn call_until<R: Request>(&self, request: R, deadline: Option<Instant>) -> Result<R::Reply> {
        let response = self.exchange_until(request.into_request(), deadline)?;
        self.inner.folding.fold(&response);
        R::Reply::read(response).map_err(|response| ClientError::Unexpected(format!("{response:?}")))
    }

    /// Sends one request; an introduction deadline ends the whole connection if its reply never arrives.
    fn exchange_until(&self, request: ClientRequest, deadline: Option<Instant>) -> Result<ClientResponse> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply_tx, reply_rx) = flume::bounded(1);
        // Before it's sent, so a reader that finishes after it was sent fails it, rather than leaving it to wait.
        self.inner.pending.lock().insert(id, reply_tx);
        match self.inner.send(id, request) {
            Ok(()) => {}
            // Its socket goes too, whether the write failed or the connection had ended already.
            Err(ClientError::Disconnected) => {
                self.disconnect();
                return Err(ClientError::Disconnected);
            }
            // One that couldn't be encoded, which leaves the connection as it was.
            Err(e) => {
                self.inner.pending.lock().remove(&id);
                return Err(e);
            }
        }
        match deadline {
            None => reply_rx.recv().map_err(|_| ClientError::Disconnected)?,
            Some(deadline) => match reply_rx.recv_deadline(deadline) {
                Ok(reply) => reply,
                Err(flume::RecvTimeoutError::Disconnected) => Err(ClientError::Disconnected),
                Err(flume::RecvTimeoutError::Timeout) => {
                    self.disconnect();
                    Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "the hub didn't finish connecting in time")
                        .into())
                }
            },
        }
    }

    pub(super) fn disconnect(&self) {
        self.closer.close();
        end_connection(&self.inner);
    }
}

fn read_frames(inner: &Arc<Inner>, stream: impl std::io::Read) {
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
        }
    }
    end_connection(inner);
}

fn end_connection(inner: &Inner) {
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

/// A channel for one subscription's items, limited to what a reader that keeps up could have waiting.
pub(super) fn subscription_channel() -> (flume::Sender<SubscriptionItem>, flume::Receiver<SubscriptionItem>) {
    flume::bounded(SUBSCRIPTION_CAPACITY)
}

/// Nothing's there, or what was there went away as we connected: a hub that's exiting can accept a
/// connection and then drop it, and so can one that loses the race with another starting at once.
pub fn hub_is_missing(error: &ClientError) -> bool {
    match error {
        ClientError::Disconnected => true,
        ClientError::Io(e) => matches!(
            e.kind(),
            std::io::ErrorKind::NotFound
                | std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::BrokenPipe
        ),
        ClientError::Api(_)
        | ClientError::Unexpected(_)
        | ClientError::FingerprintMismatch { .. }
        | ClientError::Unreachable { .. } => false,
    }
}
