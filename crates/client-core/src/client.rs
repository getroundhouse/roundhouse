//! The connection to a hub.

mod api;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use protocol::PROTOCOL_VERSION;
use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubNotice, ReplyOutcome,
};
use protocol::event::SessionEvent;
use protocol::frame;
use protocol::paths::Paths;
use protocol::tail::{StreamDelta, Tail};

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
}

impl ClientError {
    /// The hub is too old to know this request. Hide or disable whatever needed it.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, ClientError::Api(e) if e.code == ErrorCode::Unsupported)
    }
}

pub type Result<T> = std::result::Result<T, ClientError>;

/// What a subscription delivers. `Disconnected` is last. To resume, connect again and subscribe
/// from one past the last event's `seq`.
#[derive(Debug, Clone, PartialEq)]
pub enum SubscriptionItem {
    Event(SessionEvent),
    /// Replaces any streaming tail held so far.
    Tail(Tail),
    Delta(StreamDelta),
    /// Only on a feed subscription.
    Notice(HubNotice),
    Disconnected,
}

/// Dropping it tells the hub to stop sending.
pub struct Subscription {
    pub id: u64,
    pub items: flume::Receiver<SubscriptionItem>,
    connection: Arc<Inner>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.connection.subscriptions.lock().remove(&self.id);
        // Not waiting for the reply: this can run on any thread, and the reply doesn't matter.
        let _ = self.connection.send_only(ClientRequest::SessionUnsubscribe { subscription_id: self.id });
    }
}

type PendingReply = flume::Sender<std::result::Result<ClientResponse, ClientError>>;

struct Inner {
    writer: Mutex<UnixStream>,
    pending: Mutex<HashMap<u64, PendingReply>>,
    subscriptions: Mutex<HashMap<u64, flume::Sender<SubscriptionItem>>>,
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

/// Closes the socket when the last `Client` clone goes away. The reader thread can't be what keeps
/// this alive, or it would wait forever on a socket that only it was holding open.
struct Closer(UnixStream);

impl Drop for Closer {
    fn drop(&mut self) {
        let _ = self.0.shutdown(std::net::Shutdown::Both);
    }
}

/// Dropping the last clone disconnects, which also ends its subscriptions.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
    _closer: Arc<Closer>,
}

impl Client {
    /// Connects and checks the protocol version.
    pub fn connect(socket: &Path, client_name: &str) -> Result<Client> {
        let stream = UnixStream::connect(socket)?;
        let reader = stream.try_clone()?;
        let closer = Arc::new(Closer(stream.try_clone()?));
        let inner = Arc::new(Inner {
            writer: Mutex::new(stream),
            pending: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            disconnected: Mutex::new(false),
            hub_version: OnceLock::new(),
        });
        let for_reader = inner.clone();
        std::thread::Builder::new()
            .name("hub-client-reader".to_string())
            .spawn(move || read_frames(&for_reader, reader))?;
        let client = Client { inner, _closer: closer };
        let reply = client
            .call(ClientRequest::Hello { protocol_version: PROTOCOL_VERSION, client: client_name.to_string() })?;
        let ClientResponse::Hello { hub_version, .. } = reply else {
            return unexpected(&reply);
        };
        let _ = client.inner.hub_version.set(hub_version);
        Ok(client)
    }

    /// Which build the hub is, as it said when this connected.
    pub fn hub_version(&self) -> &str {
        self.inner.hub_version.get().map_or("", String::as_str)
    }

    /// Connects, starting a hub first if none is running. The hub is a separate process that
    /// outlives this client, which is why closing a client doesn't stop its agents.
    pub fn connect_or_start(paths: &Paths, daemon: &Path, client_name: &str) -> Result<Client> {
        match Client::connect(&paths.socket, client_name) {
            Ok(client) => return Ok(client),
            Err(e) if hub_is_missing(&e) => {}
            Err(e) => return Err(e),
        }
        start_hub(paths, daemon)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match Client::connect(&paths.socket, client_name) {
                Ok(client) => return Ok(client),
                Err(e) if hub_is_missing(&e) && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(e) => return Err(e),
            }
        }
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
        if let Err(e) = self.inner.writer.lock().write_all(&line) {
            self.inner.pending.lock().remove(&id);
            return Err(e.into());
        }
        reply_rx.recv().map_err(|_| ClientError::Disconnected)?
    }
}

fn unexpected<T>(response: &ClientResponse) -> Result<T> {
    Err(ClientError::Unexpected(format!("{response:?}")))
}

fn read_frames(inner: &Arc<Inner>, stream: UnixStream) {
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
    for (_, subscription) in inner.subscriptions.lock().drain() {
        let _ = subscription.send(SubscriptionItem::Disconnected);
    }
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
    if let Some(subscription) = inner.subscriptions.lock().get(&subscription_id) {
        let _ = subscription.send(item);
    }
}

/// Nothing's there, or what was there went away as we connected: a hub that's exiting can accept a
/// connection and then drop it, and so can one that loses the race with another starting at once.
fn hub_is_missing(error: &ClientError) -> bool {
    match error {
        ClientError::Disconnected => true,
        ClientError::Io(e) => matches!(
            e.kind(),
            std::io::ErrorKind::NotFound
                | std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::BrokenPipe
        ),
        ClientError::Api(_) | ClientError::Unexpected(_) => false,
    }
}

/// The daemon that ships next to `exe`. Follows symlinks first, so a hub started through an install's
/// `current` link stays on that version, whichever one the link names later.
pub fn daemon_beside(exe: &Path) -> std::io::Result<PathBuf> {
    Ok(exe.canonicalize()?.with_file_name("trestled"))
}

fn start_hub(paths: &Paths, daemon: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    use std::os::unix::process::CommandExt;
    // Only sets the mode if it creates the directory, because TRESTLE_HOME can point anywhere.
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&paths.data_dir)?;
    let log = std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(&paths.log)?;
    let mut command = Command::new(daemon);
    command
        .arg("hub")
        // The hub this client will look for, whatever this process's environment says.
        .env(protocol::paths::ENV_HOME, &paths.data_dir)
        .env(protocol::paths::ENV_WORKSPACES, &paths.workspaces_root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    // SAFETY: setsid is async-signal-safe, which is all pre_exec requires. It detaches the hub from
    // this terminal and process group, so closing the client doesn't reach it.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn().map_err(|e| std::io::Error::new(e.kind(), format!("starting {}: {e}", daemon.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_daemon_beside_a_linked_binary_is_in_the_version_the_link_named() {
        let dir = tempfile::tempdir().unwrap();
        let version = dir.path().canonicalize().unwrap().join("abc123");
        std::fs::create_dir(&version).unwrap();
        std::fs::write(version.join("trestle"), "").unwrap();
        std::os::unix::fs::symlink(&version, dir.path().join("current")).unwrap();

        let daemon = daemon_beside(&dir.path().join("current/trestle")).unwrap();

        assert_eq!(daemon, version.join("trestled"));
    }

    /// Answers a hello, then answers the next request with a reply no build of this client can read.
    fn start_hub_with_an_unreadable_reply(socket: &Path) {
        let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut requests = BufReader::new(stream.try_clone().unwrap()).lines();
            requests.next();
            let hello = ClientResponse::Hello { protocol_version: PROTOCOL_VERSION, hub_version: "older".to_string() };
            let outcome = ReplyOutcome::Response(hello);
            stream.write_all(&frame::encode(&HubFrame::Reply { id: 1, outcome }).unwrap()).unwrap();
            requests.next();
            stream
                .write_all(b"{\"frame\":\"reply\",\"id\":2,\"response\":{\"result\":\"from_a_newer_hub\"}}\n")
                .unwrap();
            // Stays connected, so that only the reply can end the call.
            requests.next();
        });
    }

    #[test]
    fn a_reply_this_client_cannot_read_fails_the_call_rather_than_hanging() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("hub.sock");
        start_hub_with_an_unreadable_reply(&socket);
        let client = Client::connect(&socket, "test").unwrap();

        let (finished, outcome) = flume::bounded(1);
        std::thread::spawn(move || finished.send(client.hub_status()));
        let outcome = outcome.recv_timeout(Duration::from_secs(10)).expect("the call hung");

        assert!(matches!(outcome, Err(ClientError::Unexpected(_))), "{outcome:?}");
    }
}
