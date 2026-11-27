use super::feed::Folding;
use super::frames::{end_connection, read_frames};
use super::older_side::OlderSide;
use super::sent_call::SentCall;
use super::subscribing::Channels;
use super::transport::{Closer, Transport};
use super::{commands, lines, queueing};
use parking_lot::Mutex;
use protocol::client::listed::Listed;
use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubNotice, Reply as _, Request,
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
    /// The hub, or the worker it asked, doesn't know this request or can't read it, such as one from before it, or a
    /// hub that doesn't know a kind of file newer than it.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, ClientError::Api(e) if e.code == ErrorCode::Unsupported)
    }

    /// The hub has no such thing, such as a file it was never given or has let go of.
    pub fn is_not_found(&self) -> bool {
        matches!(self, ClientError::Api(e) if e.code == ErrorCode::NotFound)
    }

    /// The hub's record allows no such change as it stands, such as a chat's draft changed after this change was typed.
    pub fn is_conflict(&self) -> bool {
        matches!(self, ClientError::Api(e) if e.code == ErrorCode::Conflict)
    }

    /// There's a real break between this client and the hub, so only [`HubAdmin`](super::HubAdmin) can reach it.
    pub fn is_version_mismatch(&self) -> bool {
        matches!(self, ClientError::Api(e) if e.code == ErrorCode::VersionMismatch)
    }

    /// The hub refused our pairing code or device token.
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, ClientError::Api(e) if e.code == ErrorCode::Unauthorized)
    }

    /// Trying again can't fix it, so a person has to: the hub refused this device's token, answered with another key
    /// than the one it was paired with, or is across a protocol break from this client, which is the older of the two.
    /// Anything else may be gone by the next try, such as a hub that can't be reached, a connection that went, or an
    /// older hub, which someone may update.
    pub fn needs_a_person(&self) -> bool {
        self.is_unauthorized()
            || matches!(self.older_side(), Some(OlderSide::Client { .. }))
            || matches!(self, ClientError::FingerprintMismatch { .. })
    }

    /// Why the hub didn't do it, when the only reason is that it would lose something, such as work that isn't pushed,
    /// so the person can be asked, and it's done if they say to, by asking again with force.
    pub fn would_lose(&self) -> Option<&str> {
        let ClientError::Api(e) = self else { return None };
        (e.code == ErrorCode::NeedsForce).then_some(e.message.as_str())
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
    /// A change to a record or an observation of what's followed.
    Notice(Box<HubNotice>),
    /// Where what's followed stands, as a list of all of it at its level, as [`Listed`] says.
    Listed(Box<Listed>),
    /// Only on a terminal's.
    Terminal(TerminalUpdate),
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

type PendingReply = flume::Sender<std::result::Result<ClientResponse, ClientError>>;

pub(super) struct Inner {
    pub(super) writer: Mutex<Box<dyn Write + Send>>,
    pub(super) pending: Mutex<HashMap<u64, PendingReply>>,
    pub(super) subscriptions: Mutex<Channels>,
    /// The channels of subscriptions ended because their readers fell behind.
    pub(super) lagged: Mutex<HashSet<u64>>,
    pub(super) next_id: AtomicU64,
    /// Set once the reader has finished, or a write has failed, so a later call can't wait forever.
    pub(super) disconnected: Mutex<bool>,
    /// Set by the hello, before anyone else has this connection.
    pub(super) hub_version: OnceLock<String>,
    /// The device the hub takes this connection to be on, set before anyone else has it: the one it paired as, over the
    /// network, or the one its hello named.
    pub(super) device: OnceLock<DeviceId>,
    /// What lasts beyond one connection, which a [`Hub`](crate::Hub) has each of its connections share.
    pub(super) lasting: Mutex<Lasting>,
    /// The files this connection has added to a session, or read back from one, so a prompt that carries one doesn't
    /// send its bytes again.
    pub(super) given: Mutex<HashSet<(SessionId, AttachmentId)>>,
    pub(super) folding: Folding,
}

/// What a client keeps that lasts beyond one connection, so a [`Hub`](crate::Hub) has each of its connections share
/// one: the calls started, which quitting waits for, each chat's line of prompts, and the prompts being queued.
#[derive(Clone, Default)]
pub(crate) struct Lasting {
    pub(super) running: Arc<commands::Running>,
    /// Each chat's prompts, in the order they were written.
    pub(super) lines: Arc<lines::Lines>,
    /// Each chat's prompts being queued from here that the hub hasn't answered for yet.
    pub(super) queueing: Arc<queueing::Queueing>,
}

impl Inner {
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

    /// Whether its hub is on this machine, which a client reaches by its socket, rather than over the network.
    pub fn hub_on_this_machine(&self) -> bool {
        self.closer.is_local()
    }

    pub(super) fn from_transport(transport: Transport) -> Result<Client> {
        let Transport { reader, writer, closer } = transport;
        let closer = Arc::new(closer);
        let inner = Arc::new(Inner {
            writer: Mutex::new(writer),
            pending: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(Channels::default()),
            lagged: Mutex::new(HashSet::new()),
            next_id: AtomicU64::new(1),
            disconnected: Mutex::new(false),
            hub_version: OnceLock::new(),
            device: OnceLock::new(),
            lasting: Mutex::default(),
            given: Mutex::default(),
            folding: Folding::default(),
        });
        let for_reader = inner.clone();
        std::thread::Builder::new()
            .name("hub-client-reader".to_string())
            .spawn(move || read_frames(&for_reader, reader))?;
        Ok(Client { inner, closer })
    }

    /// Has this connection share `lasting` with every other of its hub's, from now on, which a hub's link does as it
    /// carries what's open on to it.
    pub(crate) fn share(&self, lasting: &Lasting) {
        *self.inner.lasting.lock() = lasting.clone();
    }

    pub(super) fn lasting(&self) -> Lasting {
        self.inner.lasting.lock().clone()
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

    pub(crate) fn disconnect(&self) {
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
