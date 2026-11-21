use super::feed::Folding;
use super::frames::{end_connection, read_frames};
use super::sent_call::SentCall;
use super::transport::{Closer, Transport};
use super::{commands, lines, queueing};
use parking_lot::Mutex;
use protocol::client::github::GithubUpdate;
use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubNotice, Reply as _, Request, subscriptions,
};
use protocol::event::SessionEvent;
use protocol::frame;
use protocol::model::{AttachmentId, DeviceId, SessionId};
use protocol::tail::{StreamDelta, Tail};
use protocol::terminal::{TerminalPicture, TerminalUpdate};
use std::collections::{HashMap, HashSet};
use std::io::Write;
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
    /// Only on one to what GitHub says of what's shown.
    Github(GithubUpdate),
    /// Only on a terminal's that watches it.
    TerminalPicture(Box<TerminalPicture>),
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
    /// The device the hub takes this connection to be on, set before anyone else has it: the one it paired as, over the
    /// network, or the one its hello named.
    pub(super) device: OnceLock<DeviceId>,
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
            device: OnceLock::new(),
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

    /// The device the hub takes this connection to be on, which a chat's conversation is on when its record names it:
    /// the one it paired as, over the network, or the one its hello named. `None` for a client on no device, such as
    /// the command line.
    pub fn device(&self) -> Option<&DeviceId> {
        self.inner.device.get()
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

    /// Sends `request` now, in order with what's sent after it, and waits for its reply only when that's asked for,
    /// so a thread that can't wait, as a terminal's can't, has another wait.
    pub(super) fn send_call<R: Request>(&self, request: R) -> Result<SentCall<R>> {
        let reply = self.send_waited(request.into_request())?;
        Ok(SentCall::new(reply, self.clone()))
    }

    /// Sends one request, with what's to receive its reply.
    fn send_waited(&self, request: ClientRequest) -> Result<flume::Receiver<Result<ClientResponse>>> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply_tx, reply_rx) = flume::bounded(1);
        // Before it's sent, so a reader that finishes after it was sent fails it, rather than leaving it to wait.
        self.inner.pending.lock().insert(id, reply_tx);
        match self.inner.send(id, request) {
            Ok(()) => Ok(reply_rx),
            // Its socket goes too, whether the write failed or the connection had ended already.
            Err(ClientError::Disconnected) => {
                self.disconnect();
                Err(ClientError::Disconnected)
            }
            // One that couldn't be encoded, which leaves the connection as it was.
            Err(e) => {
                self.inner.pending.lock().remove(&id);
                Err(e)
            }
        }
    }

    /// Sends one request; an introduction deadline ends the whole connection if its reply never arrives.
    fn exchange_until(&self, request: ClientRequest, deadline: Option<Instant>) -> Result<ClientResponse> {
        let reply_rx = self.send_waited(request)?;
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
